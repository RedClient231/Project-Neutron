//! Application model for the UI layer.

use neutron_core::VirtualApp;
use serde::{Deserialize, Serialize};

/// UI-facing app model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppModel {
    pub id: u64,
    pub name: String,
    pub package_name: String,
    pub version: String,
    pub size_mb: f64,
    pub is_running: bool,
    pub gg_compat: bool,
}

impl From<VirtualApp> for AppModel {
    fn from(app: VirtualApp) -> Self {
        Self {
            id: app.id,
            name: app.label,
            package_name: app.package_name,
            version: app.version_name,
            size_mb: app.size_bytes as f64 / (1024.0 * 1024.0),
            is_running: app.is_running,
            gg_compat: app.gg_compat,
        }
    }
}
