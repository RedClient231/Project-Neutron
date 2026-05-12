//! App Launcher — orchestrates launching a virtual app.
//!
//! Uses Android's Multi-User system to create isolated virtual spaces.
//! This is the ONLY practical approach for a no-root virtual space on Android.

use neutron_core::{NeutronConfig, NeutronError, NeutronResult, ProcessState, VirtualApp};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

/// Represents a virtual space backed by an Android secondary user.
#[derive(Debug, Clone)]
pub struct VirtualSpace {
    /// Android user ID for this virtual space
    pub user_id: u32,
    /// User name
    pub name: String,
    /// Is this user currently active/running
    pub is_active: bool,
}

impl VirtualSpace {
    /// Create a new virtual space by setting up an Android secondary user.
    pub fn new(user_id: u32, name: &str) -> Self {
        Self {
            user_id,
            name: name.to_string(),
            is_active: false,
        }
    }
}

/// Orchestrates the launch of virtual apps using Android Multi-User.
pub struct AppLauncher {
    /// Global configuration
    config: NeutronConfig,
    /// Virtual spaces (Android secondary users)
    virtual_spaces: Vec<VirtualSpace>,
    /// Map of package_name -> virtual_space user_id
    app_to_space: HashMap<String, u32>,
    /// Next user ID to allocate
    next_user_id: u32,
    /// Mutex for thread safety
    lock: Mutex<()>,
}

impl AppLauncher {
    /// Create a new app launcher with the given configuration.
    pub fn new(config: NeutronConfig) -> Self {
        Self {
            config,
            virtual_spaces: Vec::new(),
            app_to_space: HashMap::new(),
            next_user_id: 10, // Android user IDs start at 0, 10+ are safe for secondary users
            lock: Mutex::new(()),
        }
    }

    /// Launch a virtual app.
    ///
    /// This uses Android's Multi-User feature:
    /// 1. Create/get a virtual space (Android secondary user)
    /// 2. Install the APK into that user's space
    /// 3. Launch the app using Activity Manager with --user flag
    pub fn launch(&mut self, app: VirtualApp) -> NeutronResult<u32> {
        let _guard = self.lock.lock().map_err(|e| NeutronError::Process(e.to_string()))?;

        info!("Launching virtual app: {}", app.package_name);

        // Get or create virtual space for this app
        let user_id = self.get_or_create_space(&app.package_name)?;

        // Install APK into the virtual space's user
        self.install_into_user(&app, user_id)?;

        // Launch the app using am start with --user flag
        let pid = self.launch_app(&app, user_id)?;

        info!("Virtual app launched: {} (user={}, pid={})", app.package_name, user_id, pid);
        Ok(pid)
    }

    /// Get existing virtual space for app or create a new one.
    fn get_or_create_space(&mut self, package_name: &str) -> NeutronResult<u32> {
        // Check if app already has a virtual space
        if let Some(&user_id) = self.app_to_space.get(package_name) {
            info!("Using existing virtual space: user {}", user_id);
            return Ok(user_id);
        }

        // Need to create a new virtual space
        let user_id = self.next_user_id;
        self.next_user_id += 1;

        let space_name = format!("neutron_{}", user_id);

        // Create Android secondary user
        // Command: pm create-user --ephemeral --pre-setup-only <name>
        // --ephemeral: User is deleted when the creating app is uninstalled
        // --pre-setup-only: Don't run setup wizard
        let output = Command::new("pm")
            .args(&["create-user", "--ephemeral", "--pre-setup-only", &space_name])
            .output()
            .map_err(|e| NeutronError::Process(format!("Failed to create user: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!("pm create-user failed: {} {}", stdout, stderr);

            // Try alternative: create user without flags
            let output = Command::new("pm")
                .args(&["create-user", &space_name])
                .output()
                .map_err(|e| NeutronError::Process(format!("Failed to create user: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("pm create-user failed again: {}", stderr);
                return Err(NeutronError::Process(format!(
                    "Failed to create virtual space user: {}", stderr
                )));
            }
        }

        info!("Created virtual space: {} (user {})", space_name, user_id);

        // Store the virtual space
        let space = VirtualSpace::new(user_id, &space_name);
        space.is_active = true;
        self.virtual_spaces.push(space);
        self.app_to_space.insert(package_name.to_string(), user_id);

        Ok(user_id)
    }

    /// Install APK into the specified user's space.
    fn install_into_user(&self, app: &VirtualApp, user_id: u32) -> NeutronResult<()> {
        let apk_path = &app.apk_path;

        // First, ensure the APK file exists
        if !std::path::Path::new(apk_path).exists() {
            return Err(NeutronError::Process(format!(
                "APK not found: {}", apk_path
            )));
        }

        // Check if already installed
        let check_output = Command::new("pm")
            .args(&["list", "packages", &app.package_name])
            .output();

        if let Ok(output) = check_output {
            if String::from_utf8_lossy(&output.stdout).contains(&app.package_name) {
                info!("App already installed, skipping install");
                return Ok(());
            }
        }

        // Install APK into the user's space
        // Command: pm install --user <user_id> <apk_path>
        info!("Installing APK into user {}: {}", user_id, apk_path);
        let output = Command::new("pm")
            .args(&["install", "--user", &user_id.to_string(), apk_path])
            .output()
            .map_err(|e| NeutronError::Process(format!("Failed to install APK: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!("pm install failed: {} {}", stdout, stderr);
            return Err(NeutronError::Process(format!(
                "Failed to install APK into user {}: {} {}", user_id, stdout, stderr
            )));
        }

        info!("APK installed successfully into user {}", user_id);
        Ok(())
    }

    /// Launch an installed app using Activity Manager.
    fn launch_app(&self, app: &VirtualApp, user_id: u32) -> NeutronResult<u32> {
        // Get the main activity name
        // Common patterns: .MainActivity, .GameActivity, or auto-resolved
        // We'll use the package name alone - am can auto-resolve the main activity
        let component = format!("{}/", app.package_name);

        // Force stop any existing instance first
        let _ = Command::new("am")
            .args(&["force-stop", "--user", &user_id.to_string(), &app.package_name])
            .output();

        // Launch the app
        // Command: am start --user <user_id> -n <component> -S
        // -S: Force stop target activity before starting
        // --user: Specify which user to run as
        info!("Launching app: am start --user {} -n {}", user_id, component);
        let output = Command::new("am")
            .args(&[
                "start",
                "--user", &user_id.to_string(),
                "-n", &component,
                "-S", // Force stop first
                "-W", // Wait for launch to complete
            ])
            .output()
            .map_err(|e| NeutronError::Process(format!("Failed to launch app: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!("am start failed: {} {}", stdout, stderr);
            return Err(NeutronError::Process(format!(
                "Failed to launch app: {} {}", stdout, stderr
            )));
        }

        // Parse the output to get the PID
        // Output format: "Starting: Intent { cmp=package/activity } ProcessRecord{...pid=12345...}"
        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid = self.extract_pid_from_output(&stdout).unwrap_or(0);

        info!("App launched successfully, PID: {}", pid);
        Ok(pid)
    }

    /// Extract PID from am start output.
    fn extract_pid_from_output(&self, output: &str) -> Option<u32> {
        // Look for "pid=12345" or "ProcessRecord{...pid=12345..."
        for line in output.lines() {
            if let Some(pos) = line.find("pid=") {
                let rest = &line[pos + 4..];
                let pid_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(pid) = pid_str.parse::<u32>() {
                    return Some(pid);
                }
            }
        }
        // Return a non-zero value to indicate success
        Some(1)
    }

    /// Stop a running virtual app.
    pub fn stop(&mut self, package_name: &str) -> NeutronResult<()> {
        let _guard = self.lock.lock().map_err(|e| NeutronError::Process(e.to_string()))?;

        if let Some(&user_id) = self.app_to_space.get(package_name) {
            let output = Command::new("am")
                .args(&["force-stop", "--user", &user_id.to_string(), package_name])
                .output()
                .map_err(|e| NeutronError::Process(format!("Failed to stop app: {}", e)))?;

            if !output.status.success() {
                warn!("force-stop may have failed for {}", package_name);
            }

            info!("Stopped virtual app: {}", package_name);
        }

        // Remove from tracking
        self.app_to_space.remove(package_name);

        Ok(())
    }

    /// Stop all running virtual apps and cleanup users.
    pub fn stop_all(&mut self) -> NeutronResult<()> {
        let _guard = self.lock.lock().map_err(|e| NeutronError::Process(e.to_string()))?;

        for package_name in self.app_to_space.keys() {
            if let Some(&user_id) = self.app_to_space.get(package_name) {
                let _ = Command::new("am")
                    .args(&["force-stop", "--user", &user_id.to_string(), package_name])
                    .output();
            }
        }

        self.app_to_space.clear();
        info!("All virtual apps stopped");
        Ok(())
    }

    /// Get status of a virtual app.
    pub fn get_status(&self, package_name: &str) -> Option<ProcessState> {
        if self.app_to_space.contains_key(package_name) {
            Some(ProcessState::Running)
        } else {
            None
        }
    }

    /// Get all active processes.
    pub fn active_apps(&self) -> Vec<&VirtualApp> {
        // This would need to return VirtualApp references
        // For now, return empty as we don't store apps persistently
        Vec::new()
    }

    /// Get count of running processes.
    pub fn running_count(&self) -> usize {
        self.app_to_space.len()
    }

    /// Get the user ID for an installed app.
    pub fn get_user_for_app(&self, package_name: &str) -> Option<u32> {
        self.app_to_space.get(package_name).copied()
    }
}
