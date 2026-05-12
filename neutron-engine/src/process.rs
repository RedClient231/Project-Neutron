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
            // Child process — this is the virtual app
            // In production, we'd execve() the app's entry point here.
            // For now, setup the environment and wait.
            self.setup_child_env();
            
            // Child enters the app execution loop
            // (In real implementation, this loads the DEX/native lib)
            unsafe {
                // Allow parent to trace us
                libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0);
                libc::raise(libc::SIGSTOP);
            }
            std::process::exit(0);
        }

        // Parent returns child PID
        Ok(pid as u32)
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
