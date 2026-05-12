//! # Neutron Engine — Process Virtualization Core
//!
//! Implements the virtual environment by:
//! 1. Spawning child processes via clone()
//! 2. Attaching with ptrace for syscall interception
//! 3. Redirecting filesystem, identity, and process info
//! 4. Managing lifecycle of virtualized apps

pub mod process;
pub mod tracer;
pub mod namespace;
pub mod hook;
pub mod launcher;

pub use process::VirtualProcess;
pub use tracer::SyscallTracer;
pub use namespace::VirtualNamespace;
pub use launcher::AppLauncher;
