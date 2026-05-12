//! # Neutron Core
//! 
//! Foundational types, error handling, and platform abstractions for the
//! Neutron Virtual Space engine. Written entirely in Rust + inline Assembly.
//! 
//! ## Architecture
//! - Process isolation via ptrace-based syscall interception
//! - Virtual filesystem overlay
//! - Identity spoofing (device, process, memory maps)
//! - No root required — operates within Android's user-space constraints

pub mod error;
pub mod types;
pub mod config;
pub mod platform;
pub mod syscall;

pub use error::{NeutronError, NeutronResult};
pub use types::*;
pub use config::NeutronConfig;
