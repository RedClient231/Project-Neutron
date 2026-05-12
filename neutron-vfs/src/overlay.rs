//! Virtual Filesystem Overlay
//!
//! Implements copy-on-write filesystem semantics for the virtual space.
//! All file accesses from virtualized processes are intercepted via ptrace
//! and redirected through this overlay system.

use anyhow::Result;
use log::{debug, warn};
use neutron_core::{NeutronError, NeutronResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The VFS overlay manages path mappings for a single virtual app.
#[derive(Debug)]
pub struct VfsOverlay {
    /// Package name of the virtual app
    package_name: String,
    /// Base directory for this app's virtual filesystem
    base_dir: PathBuf,
    /// Explicit path redirections (source -> target)
    redirections: HashMap<String, String>,
    /// Paths that are blocked (return ENOENT)
    blocked_paths: Vec<String>,
    /// Read-only paths (writes return EROFS)
    readonly_paths: Vec<String>,
}

impl VfsOverlay {
    /// Create a new VFS overlay for the given package.
    pub fn new(package_name: &str, base_dir: &str) -> Self {
        let app_vfs_dir = PathBuf::from(base_dir).join(package_name);
        
        Self {
            package_name: package_name.to_string(),
            base_dir: app_vfs_dir,
            redirections: Self::default_redirections(package_name, base_dir),
            blocked_paths: Self::default_blocked_paths(),
            readonly_paths: Self::default_readonly_paths(),
        }
    }

    /// Initialize the VFS directory structure on disk.
    pub fn initialize(&self) -> NeutronResult<()> {
        let dirs = [
            self.base_dir.join("data"),
            self.base_dir.join("cache"),
            self.base_dir.join("shared_prefs"),
            self.base_dir.join("databases"),
            self.base_dir.join("files"),
            self.base_dir.join("lib"),
            self.base_dir.join("oat"),
            self.base_dir.join("code_cache"),
        ];

        for dir in &dirs {
            std::fs::create_dir_all(dir)?;
        }

        debug!("VFS overlay initialized for {}", self.package_name);
        Ok(())
    }

    /// Resolve a path through the overlay. Returns the real path to use.
    pub fn resolve_path(&self, original: &str) -> PathResolution {
        // Check if path is blocked
        if self.is_blocked(original) {
            return PathResolution::Blocked;
        }

        // Check explicit redirections
        if let Some(redirect) = self.find_redirect(original) {
            return PathResolution::Redirected(redirect);
        }

        // Check if path is in virtual data directory
        if original.starts_with("/data/data/") || original.starts_with("/data/user/") {
            let virtual_path = self.redirect_data_path(original);
            return PathResolution::Redirected(virtual_path);
        }

        // /proc paths need spoofing
        if original.starts_with("/proc/") {
            return PathResolution::NeedsSpoofing(original.to_string());
        }

        // Everything else passes through
        PathResolution::Passthrough
    }

    /// Check if a file exists in our virtual overlay.
    pub fn exists_in_overlay(&self, path: &str) -> bool {
        let overlay_path = self.to_overlay_path(path);
        Path::new(&overlay_path).exists()
    }

    /// Write a file into the virtual overlay.
    pub fn write_to_overlay(&self, path: &str, data: &[u8]) -> NeutronResult<()> {
        let overlay_path = self.to_overlay_path(path);
        if let Some(parent) = Path::new(&overlay_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&overlay_path, data)?;
        Ok(())
    }

    /// Read a file from the virtual overlay.
    pub fn read_from_overlay(&self, path: &str) -> NeutronResult<Vec<u8>> {
        let overlay_path = self.to_overlay_path(path);
        Ok(std::fs::read(&overlay_path)?)
    }

    // --- Private helpers ---

    fn to_overlay_path(&self, original: &str) -> String {
        let sanitized = original.replace('/', "_");
        self.base_dir.join("overlay").join(sanitized)
            .to_string_lossy().to_string()
    }

    fn is_blocked(&self, path: &str) -> bool {
        self.blocked_paths.iter().any(|b| path.starts_with(b))
    }

    fn find_redirect(&self, path: &str) -> Option<String> {
        for (prefix, target) in &self.redirections {
            if path.starts_with(prefix) {
                let suffix = &path[prefix.len()..];
                return Some(format!("{}{}", target, suffix));
            }
        }
        None
    }

    fn redirect_data_path(&self, original: &str) -> String {
        // Map /data/data/<pkg>/... -> our virtual data dir
        let virtual_data = self.base_dir.join("data");
        if let Some(after_pkg) = original.split(&self.package_name).nth(1) {
            format!("{}{}", virtual_data.to_string_lossy(), after_pkg)
        } else {
            format!("{}/{}", virtual_data.to_string_lossy(), 
                original.rsplit('/').next().unwrap_or("unknown"))
        }
    }

    fn default_redirections(package_name: &str, base_dir: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let pkg_base = format!("{}/{}", base_dir, package_name);
        
        // Standard Android app data paths
        map.insert(
            format!("/data/data/{}", package_name),
            format!("{}/data", pkg_base),
        );
        map.insert(
            format!("/data/user/0/{}", package_name),
            format!("{}/data", pkg_base),
        );
        map.insert(
            format!("/storage/emulated/0/Android/data/{}", package_name),
            format!("{}/external", pkg_base),
        );
        
        map
    }

    fn default_blocked_paths() -> Vec<String> {
        vec![
            // Block detection of our virtual environment
            "/data/data/com.neutron.virtualspace".into(),
            "/proc/self/maps".into(), // Will be spoofed instead
            // Block root detection paths
            "/system/app/Superuser.apk".into(),
            "/system/xbin/su".into(),
            "/sbin/su".into(),
            "/data/local/su".into(),
            "/data/local/xbin/su".into(),
        ]
    }

    fn default_readonly_paths() -> Vec<String> {
        vec![
            "/system/".into(),
            "/vendor/".into(),
            "/product/".into(),
        ]
    }
}

/// Result of resolving a path through the VFS overlay.
#[derive(Debug, Clone)]
pub enum PathResolution {
    /// Path passes through to real filesystem unchanged
    Passthrough,
    /// Path is redirected to a different location
    Redirected(String),
    /// Path is blocked (return ENOENT)
    Blocked,
    /// Path needs dynamic spoofing (e.g., /proc entries)
    NeedsSpoofing(String),
    /// Path is read-only (block writes)
    ReadOnly,
}
