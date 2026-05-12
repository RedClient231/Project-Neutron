//! # Neutron VFS — Virtual Filesystem
//!
//! Provides filesystem isolation for virtual apps. Uses an overlay approach:
//! - Reads fall through to the real filesystem for system files
//! - Writes are redirected to app-private virtual storage
//! - Sensitive paths (/proc, /sys, /data) are intercepted and spoofed

pub mod overlay;
pub mod redirect;
pub mod procfs;

pub use overlay::VfsOverlay;
pub use redirect::PathRedirector;
pub use procfs::ProcfsSpoofing;
