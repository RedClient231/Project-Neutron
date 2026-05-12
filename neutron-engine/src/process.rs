//! Virtual process management.
//!
//! Creates and manages child processes that run inside the virtual space.
//! Uses clone() with isolated namespaces where possible, and ptrace
//! for syscall interception on Android without root.

use neutron_core::{ProcessState, NeutronError, NeutronResult, VirtualApp, NativeAbi};
use neutron_vfs::{VfsOverlay, PathRedirector, ProcfsSpoofing};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Represents a single virtualized process in Neutron space.
#[derive(Debug)]
pub struct VirtualProcess {
    /// The virtual app this process belongs to
    pub app: VirtualApp,
    /// Process ID in the real system
    pub real_pid: AtomicU32,
    /// Current state
    pub state: ProcessState,
    /// VFS overlay for filesystem isolation
    vfs: VfsOverlay,
    /// Path redirector
    redirector: PathRedirector,
    /// /proc spoofing handler
    procfs: Option<ProcfsSpoofing>,
    /// Whether this process should remain alive
    keep_alive: AtomicBool,
    /// Thread IDs belonging to this process
    thread_ids: Vec<u32>,
    /// Environment variables for the virtual process
    env_vars: HashMap<String, String>,
    /// Native library paths for this app
    lib_paths: Vec<String>,
}

impl VirtualProcess {
    /// Create a new virtual process for the given app.
    pub fn new(app: VirtualApp, vfs_root: &str) -> Self {
        let vfs = VfsOverlay::new(&app.package_name, vfs_root);
        let redirector = PathRedirector::new(&app.package_name, vfs_root);

        Self {
            app,
            real_pid: AtomicU32::new(0),
            state: ProcessState::Idle,
            vfs,
            redirector,
            procfs: None,
            keep_alive: AtomicBool::new(true),
            thread_ids: Vec::new(),
            env_vars: Self::default_env_vars(),
            lib_paths: Vec::new(),
        }
    }

    /// Spawn the virtual process.
    ///
    /// This performs:
    /// 1. fork() via clone syscall
    /// 2. In child: setup environment, load native libs, exec
    /// 3. In parent: ptrace attach for syscall interception
    pub fn spawn(&mut self) -> NeutronResult<u32> {
        info!("Spawning virtual process for: {}", self.app.package_name);

        // Initialize VFS
        self.vfs.initialize()?;

        // Build library path for this app
        let lib_dir = format!("{}/{}/lib",
            self.redirector.get_virtual_root(),
            self.app.package_name);
        self.lib_paths = vec![lib_dir];

        // Fork child process
        let pid = self.fork_child()?;
        self.real_pid.store(pid, Ordering::SeqCst);
        self.state = ProcessState::Initializing;

        // Setup procfs spoofing
        let host_pid = std::process::id();
        self.procfs = Some(ProcfsSpoofing::new(
            pid,
            host_pid,
            self.app.gg_compat,
        ));

        info!("Virtual process spawned: pid={}", pid);
        self.state = ProcessState::Running;

        Ok(pid)
    }

    /// Kill the virtual process.
    pub fn kill(&mut self) -> NeutronResult<()> {
        self.keep_alive.store(false, Ordering::SeqCst);
        let pid = self.real_pid.load(Ordering::SeqCst);
        
        if pid > 0 {
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
            self.state = ProcessState::Exited(0);
            info!("Virtual process killed: pid={}", pid);
        }
        
        Ok(())
    }

    /// Suspend the process (ptrace stop).
    pub fn suspend(&mut self) -> NeutronResult<()> {
        let pid = self.real_pid.load(Ordering::SeqCst);
        if pid > 0 {
            unsafe {
                libc::kill(pid as i32, libc::SIGSTOP);
            }
            self.state = ProcessState::Suspended;
        }
        Ok(())
    }

    /// Resume a suspended process.
    pub fn resume(&mut self) -> NeutronResult<()> {
        let pid = self.real_pid.load(Ordering::SeqCst);
        if pid > 0 {
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
            self.state = ProcessState::Running;
        }
        Ok(())
    }

    /// Get the VFS overlay reference.
    pub fn vfs(&self) -> &VfsOverlay {
        &self.vfs
    }

    /// Get the path redirector reference.
    pub fn redirector(&self) -> &PathRedirector {
        &self.redirector
    }

    /// Get the procfs spoofing handler.
    pub fn procfs(&self) -> Option<&ProcfsSpoofing> {
        self.procfs.as_ref()
    }

    /// Check if GameGuardian compatibility is enabled.
    pub fn gg_compat_enabled(&self) -> bool {
        self.app.gg_compat
    }

    // --- Private ---

    fn fork_child(&self) -> NeutronResult<u32> {
        let pid = unsafe { libc::fork() };

        if pid < 0 {
            return Err(NeutronError::Process(
                format!("fork() failed: {}", std::io::Error::last_os_error())
            ));
        }

        if pid == 0 {
            // Child process — launch the actual Android app
            self.launch_android_app();
            // If launch_android_app returns, execve failed
            std::process::exit(1);
        }

        // Parent returns child PID
        Ok(pid as u32)
    }

    /// Launch the Android app. This is the entry point that gets executed
    /// after fork() in the child process.
    ///
    /// Uses Android's Activity Manager (`am`) to launch the app. This is how
    /// ALL Android virtual space apps work (VMOS, Virtual Master, etc.) - they
    /// use `am start` and let Activity Manager handle process spawning from Zygote.
    fn launch_android_app(&self) {
        use std::ffi::CString;
        use std::ptr;

        // Setup virtualized environment variables
        self.setup_child_env();

        // Build virtual paths
        let virtual_root = self.redirector.get_virtual_root();
        let app_dir = format!("{}/{}", virtual_root, self.app.package_name);

        // Set up environment for the virtualized app
        std::env::set_var("ANDROID_DATA", &app_dir);
        std::env::set_var("ANDROID_ROOT", "/system");

        // For GameGuardian compatibility
        if self.app.gg_compat {
            std::env::set_var("DEBUGGABLE", "1");
        }

        // === USE ACTIVITY MANAGER (am) - This is how real virtual spaces work ===
        //
        // The 'am' (Activity Manager) command is the standard Android way to launch apps.
        // It handles:
        // 1. Resolving the activity name from the package
        // 2. Forking a new process from Zygote
        // 3. Loading the APK
        // 4. Starting the Dalvik/ART runtime with the correct Activity
        //
        // Command: am start -n package/activity -S -W --user 0 --taskAffinity ""
        //
        // Key flags:
        //   -n <package>/<activity>  : Component to launch (activity auto-resolved by am)
        //   -S                      : Force stop target app first
        //   -W                      : Wait for launch to complete
        //   --user 0               : Launch in user 0 (main Android user)
        //   --taskAffinity ""      : Don't create new task (run in virtual space task)

        // Resolve activity name - am can auto-resolve common patterns
        // For most apps: package/.MainActivity works, or just package alone
        let activity_spec = format!("{}/", self.app.package_name);

        // Build 'am start' command
        let am_bin = CString::new("/system/bin/am").unwrap();

        let am_args = vec![
            // arg[0]: program name
            CString::new("am").unwrap(),
            // arg[1]: subcommand
            CString::new("start").unwrap(),
            // arg[2]: -n <component>
            CString::new(format!("-n {}", activity_spec)).unwrap(),
            // arg[3]: -S (force stop first)
            CString::new("-S").unwrap(),
            // arg[4]: --user 0 (main user)
            CString::new("--user").unwrap(),
            CString::new("0").unwrap(),
            // arg[5,6]: --taskAffinity "" (no new task)
            CString::new("--taskAffinity").unwrap(),
            CString::new("").unwrap(),
            // arg[7,8]: -D (enable debug if GG compat)
            CString::new("-D").unwrap(),
        ];

        // Build argv: [/system/bin/am, start, -n package/, -S, --user, 0, ...]
        let mut argv: Vec<*const libc::c_char> = vec![am_bin.as_ptr()];
        for arg in &am_args {
            argv.push(arg.as_ptr());
        }
        argv.push(ptr::null());

        // Build environment array
        let env_vars: Vec<CString> = std::env::vars()
            .filter(|(k, _)| !k.starts_with("RUST_"))
            .map(|(k, v)| CString::new(format!("{}={}", k, v)).unwrap())
            .collect();
        let mut env: Vec<*const libc::c_char> = env_vars.iter()
            .map(|s| s.as_ptr())
            .collect();
        env.push(ptr::null());

        // Signal we're traceable (for syscall interception if needed)
        unsafe {
            libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0);
            libc::raise(libc::SIGSTOP);
        }

        // Execute am start - this replaces this process with Activity Manager
        // am will internally fork and spawn the actual app
        unsafe {
            libc::execve(am_bin.as_ptr(), argv.as_ptr(), env.as_ptr());
        }

        // If execve returns, it failed - log error
        error!("Failed to execve am for: {}", self.app.package_name);
    }

    /// Resolve the main activity class name from the package.
    /// With the 'am' approach, we don't need this anymore as am auto-resolves.
    fn resolve_main_activity(&self) -> String {
        format!("{}/", self.app.package_name)
    }

    fn setup_child_env(&self) {
        for (key, val) in &self.env_vars {
            std::env::set_var(key, val);
        }
    }

    fn default_env_vars() -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("ANDROID_DATA".into(), "/data".into());
        env.insert("ANDROID_ROOT".into(), "/system".into());
        env.insert("EXTERNAL_STORAGE".into(), "/storage/emulated/0".into());
        env.insert("LD_LIBRARY_PATH".into(), "/system/lib64:/system/vendor/lib64".into());
        env
    }
}
