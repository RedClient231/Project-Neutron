//! # Neutron Compat — Compatibility Layer
//!
//! Provides:
//! - 32-bit / 64-bit game compatibility
//! - GameGuardian support (memory reading, process attachment)
//! - GPU passthrough for Mali (Helio G99)
//! - Anti-detection evasion

pub mod game_compat;
pub mod gameguardian;
pub mod gpu;
pub mod anti_detect;

pub use game_compat::GameCompat;
pub use gameguardian::GameGuardianSupport;
pub use gpu::GpuPassthrough;
pub use anti_detect::AntiDetection;
