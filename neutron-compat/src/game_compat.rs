//! Game compatibility layer for 32-bit and 64-bit native games.
//!
//! Handles:
//! - Proper linker namespace setup for native libraries
//! - ELF loading assistance
//! - Memory layout requirements for games
//! - Mali GPU driver compatibility

use neutron_core::{NativeAbi, NeutronError, NeutronResult};
use log::{debug, info, warn};
use std::path::Path;

/// Game compatibility handler.
pub struct GameCompat {
    /// Target ABI for the current game
    abi: NativeAbi,
    /// Whether 32-bit emulation is needed on 64-bit host
    needs_32bit_compat: bool,
    /// Library search paths
    lib_paths: Vec<String>,
    /// Preloaded libraries
    preloads: Vec<String>,
}

impl GameCompat {
    /// Create a new game compatibility handler.
    pub fn new(abi: NativeAbi) -> Self {
        let needs_32bit = abi == NativeAbi::ArmeabiV7a;
        
        Self {
            abi,
            needs_32bit_compat: needs_32bit,
            lib_paths: Self::default_lib_paths(abi),
            preloads: Vec::new(),
        }
    }

    /// Setup the library search path for the game.
    pub fn setup_lib_paths(&mut self, app_lib_dir: &str) {
        // App's own native libs come first
        self.lib_paths.insert(0, app_lib_dir.to_string());
    }

    /// Get the LD_LIBRARY_PATH for the virtual process.
    pub fn library_path(&self) -> String {
        self.lib_paths.join(":")
    }

    /// Verify a native library is compatible with the target ABI.
    pub fn verify_elf_compat(&self, lib_path: &str) -> NeutronResult<bool> {
        let data = std::fs::read(lib_path)?;
        
        if data.len() < 20 {
            return Err(NeutronError::NativeLib {
                path: lib_path.into(),
                reason: "File too small for ELF".into(),
            });
        }

        // Check ELF magic
        if &data[0..4] != b"\x7fELF" {
            return Err(NeutronError::NativeLib {
                path: lib_path.into(),
                reason: "Not an ELF file".into(),
            });
        }

        // Check class (32 vs 64 bit)
        let elf_class = data[4];
        let expected_class = match self.abi {
            NativeAbi::Arm64V8a | NativeAbi::Universal => 2, // ELFCLASS64
            NativeAbi::ArmeabiV7a => 1, // ELFCLASS32
        };

        if elf_class != expected_class {
            warn!("ELF class mismatch: {} has class {}, expected {}", 
                lib_path, elf_class, expected_class);
            return Ok(false);
        }

        // Check machine type
        let machine = if elf_class == 2 {
            u16::from_le_bytes([data[18], data[19]])
        } else {
            u16::from_le_bytes([data[18], data[19]])
        };

        let expected_machine = self.abi.elf_machine();
        if machine != expected_machine {
            warn!("ELF machine mismatch: {} has {}, expected {}", 
                lib_path, machine, expected_machine);
            return Ok(false);
        }

        Ok(true)
    }

    /// Scan app's native library directory and verify all libs.
    pub fn verify_all_libs(&self, lib_dir: &str) -> NeutronResult<Vec<String>> {
        let mut incompatible = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "so").unwrap_or(false) {
                    let path_str = path.to_string_lossy().to_string();
                    match self.verify_elf_compat(&path_str) {
                        Ok(false) => incompatible.push(path_str),
                        Err(e) => {
                            warn!("Failed to verify {}: {}", path_str, e);
                            incompatible.push(path_str);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(incompatible)
    }

    /// Get required environment variables for game execution.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        let mut vars = vec![
            ("LD_LIBRARY_PATH".into(), self.library_path()),
        ];

        if self.needs_32bit_compat {
            vars.push(("ANDROID_EXECUTION_MODE".into(), "32".into()));
        }

        // Mali GPU specific
        vars.push(("MALI_VISIBLE_DEVICE".into(), "0".into()));
        vars.push(("GPU_DEBUG_LAYER".into(), "0".into()));

        vars
    }

    /// Check if 32-bit compatibility layer is needed.
    pub fn needs_32bit_layer(&self) -> bool {
        self.needs_32bit_compat
    }

    // --- Private ---

    fn default_lib_paths(abi: NativeAbi) -> Vec<String> {
        match abi {
            NativeAbi::Arm64V8a | NativeAbi::Universal => vec![
                "/system/lib64".into(),
                "/system/vendor/lib64".into(),
                "/system/lib64/hw".into(),
                "/vendor/lib64".into(),
                "/vendor/lib64/hw".into(),
                "/vendor/lib64/egl".into(),
            ],
            NativeAbi::ArmeabiV7a => vec![
                "/system/lib".into(),
                "/system/vendor/lib".into(),
                "/system/lib/hw".into(),
                "/vendor/lib".into(),
                "/vendor/lib/hw".into(),
                "/vendor/lib/egl".into(),
            ],
        }
    }
}
