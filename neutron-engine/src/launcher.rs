//! App Launcher — orchestrates launching a virtual app.
//!
//! Coordinates between the process manager, syscall tracer, VFS,
//! and compatibility layers to start a full virtual app.

use neutron_core::{NeutronConfig, NeutronError, NeutronResult, ProcessState, VirtualApp};
use neutron_vfs::{PathRedirector, ProcfsSpoofing, VfsOverlay};
use log::{debug, error, info};

use crate::process::VirtualProcess;
use crate::tracer::SyscallTracer;
use crate::namespace::VirtualNamespace;

/// Orchestrates the launch of virtual apps.
pub struct AppLauncher {
    /// Global configuration
    config: NeutronConfig,
    /// Active virtual processes
    active_processes: Vec<VirtualProcess>,
    /// Virtual namespaces
    namespaces: Vec<VirtualNamespace>,
}

impl AppLauncher {
    /// Create a new app launcher with the given configuration.
    pub fn new(config: NeutronConfig) -> Self {
        Self {
            config,
            active_processes: Vec::new(),
            namespaces: Vec::new(),
        }
    }

    /// Launch a virtual app.
    ///
    /// This sets up the entire virtual environment:
    /// 1. Creates the virtual namespace
    /// 2. Initializes the VFS overlay
    /// 3. Spawns the child process
    /// 4. Attaches the syscall tracer
    /// 5. Starts the tracing loop in a background thread
    pub fn launch(&mut self, app: VirtualApp) -> NeutronResult<u32> {
        info!("Launching virtual app: {}", app.package_name);

        // Create namespace
        let uid = 10000 + self.namespaces.len() as u32;
        let mut namespace = VirtualNamespace::new(&app.package_name, uid);
        
        // Set identity from config
        namespace.set_identity(self.config.identity.clone());

        // Create and spawn virtual process
        let mut process = VirtualProcess::new(app.clone(), &self.config.vfs_dir);
        
        let lib_paths = vec![
            format!("{}/{}/lib", self.config.apps_dir, app.package_name),
        ];
        
        let pid = process.spawn(&lib_paths)?;
        
        // Register in namespace
        namespace.register_process(pid);

        // Store
        self.namespaces.push(namespace);
        self.active_processes.push(process);

        info!("Virtual app launched: {} (pid={})", app.package_name, pid);
        Ok(pid)
    }

    /// Stop a running virtual app.
    pub fn stop(&mut self, package_name: &str) -> NeutronResult<()> {
        if let Some(proc) = self.active_processes.iter_mut()
            .find(|p| p.app.package_name == package_name)
        {
            proc.kill()?;
            info!("Stopped virtual app: {}", package_name);
        }
        
        // Remove from active list
        self.active_processes.retain(|p| p.app.package_name != package_name);
        
        Ok(())
    }

    /// Stop all running virtual apps.
    pub fn stop_all(&mut self) -> NeutronResult<()> {
        for proc in &mut self.active_processes {
            let _ = proc.kill();
        }
        self.active_processes.clear();
        self.namespaces.clear();
        Ok(())
    }

    /// Get status of a virtual app.
    pub fn get_status(&self, package_name: &str) -> Option<ProcessState> {
        self.active_processes.iter()
            .find(|p| p.app.package_name == package_name)
            .map(|p| p.state)
    }

    /// Get all active processes.
    pub fn active_apps(&self) -> Vec<&VirtualApp> {
        self.active_processes.iter().map(|p| &p.app).collect()
    }

    /// Get count of running processes.
    pub fn running_count(&self) -> usize {
        self.active_processes.iter()
            .filter(|p| matches!(p.state, ProcessState::Running))
            .count()
    }
}
