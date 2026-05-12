//! # Neutron APK — APK/XAPK Parser and Installer
//!
//! Handles parsing, extracting, and installing Android packages
//! into the Neutron virtual space. Supports:
//! - Standard APK files
//! - XAPK bundles (split APKs)
//! - OBB files
//! - Native library extraction (arm64-v8a, armeabi-v7a)

pub mod parser;
pub mod installer;
pub mod manifest;
pub mod xapk;

pub use parser::ApkParser;
pub use installer::ApkInstaller;
pub use manifest::ManifestInfo;
pub use xapk::XapkParser;
