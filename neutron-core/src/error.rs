//! Error types for the Neutron virtual space engine.

use thiserror::Error;

/// Core error type for all Neutron operations.
#[derive(Error, Debug)]
pub enum NeutronError {
    #[error("Process error: {0}")]
    Process(String),

    #[error("Virtual filesystem error: {0}")]
    Vfs(String),

    #[error("APK parsing error: {0}")]
    ApkParse(String),

    #[error("Syscall interception error: {0}")]
    Syscall(String),

    #[error("Memory mapping error: {0}")]
    Memory(String),

    #[error("Permission denied: {0}")]
    Permission(String),

    #[error("Compatibility layer error: {0}")]
    Compat(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Ptrace error: pid={pid}, request={request}, errno={errno}")]
    Ptrace {
        pid: i32,
        request: i64,
        errno: i32,
    },

    #[error("Namespace isolation failed: {0}")]
    Namespace(String),

    #[error("Hook installation failed at address 0x{addr:x}: {reason}")]
    Hook { addr: u64, reason: String },

    #[error("Native library load failed: {path}: {reason}")]
    NativeLib { path: String, reason: String },

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Convenience Result type for Neutron operations.
pub type NeutronResult<T> = Result<T, NeutronError>;
