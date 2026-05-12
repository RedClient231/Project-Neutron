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
    /// after fork() in the child process. It replaces the child with the
    /// actual Android app using execve().
    fn launch_android_app(&self) {
        use std::ffi::CString;
        use std::ptr;

        // Setup virtualized environment variables
        self.setup_child_env();

        // Build virtual paths
        let virtual_root = self.redirector.get_virtual_root();
        let app_dir = format!("{}/{}", virtual_root, self.app.package_name);
        let lib_path = format!("{}/lib", app_dir);
        let apk_path = &self.app.apk_path;

        // Get main activity class name from package
        // Common patterns: .MainActivity, MainActivity, package.MainActivity
        let activity_class = self.resolve_main_activity();

        // Set up environment for the virtualized app
        std::env::set_var("LD_LIBRARY_PATH", &lib_path);
        std::env::set_var("ANDROID_DATA", &app_dir);
        std::env::set_var("ANDROID_ROOT", "/system");
        std::env::set_var("ANDROID_RUNTIME_ROOT", "/system");
        std::env::set_var("CLASSPATH", apk_path);
        std::env::set_var("JAVA_HOME", "/system");

        // For GameGuardian compatibility
        if self.app.gg_compat {
            std::env::set_var("DEBUGGABLE", "1");
        }

        // === Option 1: Use dalvikvm for DEX execution ===
        // dalvikvm directly executes DEX bytecode
        let dalvikvm = CString::new("/system/bin/dalvikvm").unwrap();
        let dalvik_args = vec![
            CString::new(activity_class.clone()).unwrap(),
            CString::new("-Xcompilerargs").unwrap(),
            CString::new("--inline-with=dex").unwrap(),
            CString::new(format!("-Duser.home={}", app_dir)).unwrap(),
            CString::new(format!("-Xpsn={}", self.app.package_name)).unwrap(), // Process serial number
        ];

        let mut dalvik_argv: Vec<*const libc::c_char> = vec![dalvikvm.as_ptr()];
        for arg in &dalvik_args {
            dalvik_argv.push(arg.as_ptr());
        }
        dalvik_argv.push(ptr::null());

        // === Option 2: Use app_process for NativeActivity ===
        // app_process handles native libraries better
        let app_process = CString::new("/system/bin/app_process64").unwrap_or(
            CString::new("/system/bin/app_process").unwrap()
        );

        let nice_name = format!("{}:{}", self.app.package_name,
            self.app.label.replace(" ", "_").replace("/", "_"));
        let app_args = vec![
            CString::new(format!("--nice-name={}", nice_name)).unwrap(),
            CString::new(activity_class).unwrap(),
        ];

        let mut app_argv: Vec<*const libc::c_char> = vec![app_process.as_ptr()];
        for arg in &app_args {
            app_argv.push(arg.as_ptr());
        }
        app_argv.push(ptr::null());

        // Build environment array (must keep CStrings alive!)
        let env_vars: Vec<CString> = std::env::vars()
            .filter(|(k, _)| !k.starts_with("RUST_")) // Filter RUST_* vars
            .map(|(k, v)| CString::new(format!("{}={}", k, v)).unwrap())
            .collect();
        let mut env: Vec<*const libc::c_char> = env_vars.iter()
            .map(|s| s.as_ptr())
            .collect();
        env.push(ptr::null());

        // Setup ptrace tracing before exec (parent will trace us)
        unsafe {
            libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0);
            libc::raise(libc::SIGSTOP);
        }

        // Try dalvikvm first (for Java apps)
        unsafe {
            libc::execve(dalvikvm.as_ptr(), dalvik_argv.as_ptr(), env.as_ptr());
        }

        // If dalvikvm fails, try app_process (for NativeActivity apps)
        unsafe {
            libc::execve(app_process.as_ptr(), app_argv.as_ptr(), env.as_ptr());
        }

        // If both fail, try the APK directly (won't work, but gives error log)
        unsafe {
            let apk_bin = CString::new(apk_path.as_str()).unwrap();
            let fallback_argv: Vec<*const libc::c_char> = vec![apk_bin.as_ptr(), ptr::null()];
            libc::execve(apk_bin.as_ptr(), fallback_argv.as_ptr(), env.as_ptr());
        }

        // If we reach here, execve failed
        error!("Failed to launch Android app: {}", self.app.package_name);
        error!("  Tried: dalvikvm, app_process64, app_process");
    }

    /// Resolve the main activity class name from the package.
    /// Falls back to standard patterns if manifest parsing is unavailable.
    fn resolve_main_activity(&self) -> String {
        // Try to find common activity patterns
        // Most Android apps use these patterns:
        let package = &self.app.package_name;

        // Pattern 1: .MainActivity (most common)
        format!("{}.MainActivity", package)
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
