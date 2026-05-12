//! Configuration for the Neutron virtual space engine.

use serde::{Deserialize, Serialize};
use crate::types::VirtualIdentity;

/// Main configuration for the Neutron virtual environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronConfig {
    /// Base directory for all virtual space data
    pub data_dir: String,
    /// Directory for installed virtual APKs
    pub apps_dir: String,
    /// Directory for virtual filesystem overlays
    pub vfs_dir: String,
    /// Virtual device identity
    pub identity: VirtualIdentity,
    /// Enable GameGuardian compatibility globally
    pub gg_compat_enabled: bool,
    /// Enable /proc/pid/maps spoofing
    pub maps_spoofing: bool,
    /// Enable ptrace-based memory access for tools
    pub allow_ptrace_attach: bool,
    /// Maximum concurrent virtual processes
    pub max_processes: u32,
    /// Enable 32-bit compatibility layer
    pub enable_32bit: bool,
    /// GPU passthrough mode for Mali
    pub gpu_passthrough: bool,
}

impl Default for NeutronConfig {
    fn default() -> Self {
        Self {
            data_dir: "/data/data/com.neutron.virtualspace/files".into(),
            apps_dir: "/data/data/com.neutron.virtualspace/files/apps".into(),
            vfs_dir: "/data/data/com.neutron.virtualspace/files/vfs".into(),
            identity: VirtualIdentity::default(),
            gg_compat_enabled: true,
            maps_spoofing: true,
            allow_ptrace_attach: true,
            max_processes: 8,
            enable_32bit: true,
            gpu_passthrough: true,
        }
    }
}

impl NeutronConfig {
    /// Load configuration from the app's private storage.
    pub fn load_or_default(data_dir: &str) -> Self {
        let config_path = format!("{}/neutron_config.json", data_dir);
        match std::fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => {
                let config = Self {
                    data_dir: data_dir.into(),
                    apps_dir: format!("{}/apps", data_dir),
                    vfs_dir: format!("{}/vfs", data_dir),
                    ..Default::default()
                };
                // Attempt to persist default config
                if let Ok(json) = serde_json::to_string_pretty(&config) {
                    let _ = std::fs::create_dir_all(data_dir);
                    let _ = std::fs::write(&config_path, json);
                }
                config
            }
        }
    }

    /// Persist current configuration to disk.
    pub fn save(&self) -> crate::NeutronResult<()> {
        let config_path = format!("{}/neutron_config.json", self.data_dir);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, json)?;
        Ok(())
    }
}
