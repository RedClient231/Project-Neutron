//! Core types used throughout the Neutron virtual space engine.

use serde::{Deserialize, Serialize};

/// Represents a virtualized application installed in Neutron space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualApp {
    /// Unique identifier within Neutron
    pub id: u64,
    /// Original package name (e.g., "com.example.game")
    pub package_name: String,
    /// Display label
    pub label: String,
    /// Path to the installed APK within virtual storage
    pub apk_path: String,
    /// Native library ABI (arm64-v8a, armeabi-v7a)
    pub abi: NativeAbi,
    /// Version code from the original APK
    pub version_code: u64,
    /// Version name from the original APK
    pub version_name: String,
    /// Whether the app is currently running
    pub is_running: bool,
    /// PID of the virtualized process (0 if not running)
    pub pid: u32,
    /// Installation timestamp (unix epoch seconds)
    pub installed_at: u64,
    /// Size in bytes of the APK
    pub size_bytes: u64,
    /// Paths to split APKs (for XAPK/bundles)
    pub split_apks: Vec<String>,
    /// Whether GameGuardian compatibility mode is enabled
    pub gg_compat: bool,
}

/// Supported native ABIs for game compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeAbi {
    /// 64-bit ARM (aarch64)
    Arm64V8a,
    /// 32-bit ARM (armv7)
    ArmeabiV7a,
    /// Both 32 and 64-bit support
    Universal,
}

impl NativeAbi {
    /// Returns the Android lib directory name for this ABI.
    pub fn lib_dir_name(&self) -> &str {
        match self {
            Self::Arm64V8a => "arm64-v8a",
            Self::ArmeabiV7a => "armeabi-v7a",
            Self::Universal => "arm64-v8a",
        }
    }

    /// Returns the ELF machine type expected for this ABI.
    pub fn elf_machine(&self) -> u16 {
        match self {
            Self::Arm64V8a | Self::Universal => 183, // EM_AARCH64
            Self::ArmeabiV7a => 40,                   // EM_ARM
        }
    }
}

/// Process state within the virtual environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    /// Not started
    Idle,
    /// Being initialized (setting up namespace)
    Initializing,
    /// Running normally
    Running,
    /// Suspended (ptrace stop)
    Suspended,
    /// Process has exited
    Exited(i32),
    /// Process crashed
    Crashed(i32),
}

/// Memory region descriptor for virtual /proc/pid/maps spoofing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub permissions: String,
    pub offset: u64,
    pub device: String,
    pub inode: u64,
    pub pathname: String,
}

/// Virtual device identity for spoofing detection evasion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualIdentity {
    pub device_model: String,
    pub device_manufacturer: String,
    pub android_id: String,
    pub build_fingerprint: String,
    pub serial_number: String,
    pub imei: String,
}

impl Default for VirtualIdentity {
    fn default() -> Self {
        Self {
            device_model: "Pixel 7 Pro".into(),
            device_manufacturer: "Google".into(),
            android_id: "a1b2c3d4e5f6a7b8".into(),
            build_fingerprint: "google/cheetah/cheetah:13/TP1A.220624.021/8877034:user/release-keys".into(),
            serial_number: "NEUTRON00001".into(),
            imei: "000000000000000".into(),
        }
    }
}

/// Syscall filter action for the interception layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallAction {
    /// Allow the syscall to proceed unmodified
    Allow,
    /// Block the syscall and return the specified errno
    Block(i32),
    /// Redirect to our virtual implementation
    Redirect,
    /// Modify arguments before execution
    ModifyArgs,
    /// Spoof the return value after execution
    SpoofReturn(i64),
}

/// Import source for installing apps into the virtual space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportSource {
    /// Import from file manager (path to APK/XAPK)
    FilePath(String),
    /// Clone from installed system app
    CloneInstalled(String),
    /// Import from content URI
    ContentUri(String),
}
