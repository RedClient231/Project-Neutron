//! APK/XAPK installer — installs packages into the virtual space.

use neutron_core::{ImportSource, NativeAbi, NeutronError, NeutronResult, VirtualApp};
use neutron_vfs::VfsOverlay;
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::parser::{ApkMetadata, ApkParser};
use crate::xapk::XapkParser;

/// Installs APK/XAPK files into the Neutron virtual space.
pub struct ApkInstaller {
    /// Base directory for installed apps
    apps_dir: String,
    /// VFS root directory
    vfs_dir: String,
    /// Next app ID to assign
    next_id: u64,
}

impl ApkInstaller {
    /// Create a new installer.
    pub fn new(apps_dir: &str, vfs_dir: &str) -> Self {
        Self {
            apps_dir: apps_dir.to_string(),
            vfs_dir: vfs_dir.to_string(),
            next_id: 1,
        }
    }

    /// Install an app from the given import source.
    pub fn install(&mut self, source: ImportSource, gg_compat: bool) -> NeutronResult<VirtualApp> {
        match source {
            ImportSource::FilePath(path) => {
                if path.ends_with(".xapk") || path.ends_with(".apks") {
                    self.install_xapk(&path, gg_compat)
                } else if path.ends_with(".apk") {
                    self.install_apk(&path, gg_compat)
                } else {
                    Err(NeutronError::ApkParse(
                        format!("Unsupported file format: {}", path)
                    ))
                }
            }
            ImportSource::CloneInstalled(package_name) => {
                self.clone_installed_app(&package_name, gg_compat)
            }
            ImportSource::ContentUri(uri) => {
                Err(NeutronError::ApkParse(
                    "Content URI import not yet implemented".into()
                ))
            }
        }
    }

    /// Install a standard APK file.
    pub fn install_apk(&mut self, apk_path: &str, gg_compat: bool) -> NeutronResult<VirtualApp> {
        info!("Installing APK: {}", apk_path);

        // Validate
        if !ApkParser::validate(apk_path)? {
            return Err(NeutronError::ApkParse("Invalid APK file".into()));
        }

        // Parse metadata
        let metadata = ApkParser::parse(apk_path)?;

        // Create app directory
        let app_dir = PathBuf::from(&self.apps_dir).join(&metadata.package_name);
        std::fs::create_dir_all(&app_dir)?;

        // Copy APK to app directory
        let dest_apk = app_dir.join("base.apk");
        std::fs::copy(apk_path, &dest_apk)?;

        // Extract native libraries
        let lib_dir = app_dir.join("lib");
        std::fs::create_dir_all(&lib_dir)?;
        ApkParser::extract_native_libs(apk_path, &lib_dir, metadata.abi)?;

        // Initialize VFS overlay
        let vfs = VfsOverlay::new(&metadata.package_name, &self.vfs_dir);
        vfs.initialize()?;

        // Create VirtualApp record
        let app = VirtualApp {
            id: self.next_id,
            package_name: metadata.package_name,
            label: metadata.label,
            apk_path: dest_apk.to_string_lossy().to_string(),
            abi: metadata.abi,
            version_code: metadata.version_code,
            version_name: metadata.version_name,
            is_running: false,
            pid: 0,
            installed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            size_bytes: metadata.total_size,
            split_apks: Vec::new(),
            gg_compat,
        };

        self.next_id += 1;
        info!("APK installed: {} ({})", app.label, app.package_name);
        Ok(app)
    }

    /// Install an XAPK bundle.
    pub fn install_xapk(&mut self, xapk_path: &str, gg_compat: bool) -> NeutronResult<VirtualApp> {
        info!("Installing XAPK: {}", xapk_path);

        // Validate
        if !XapkParser::validate(xapk_path)? {
            return Err(NeutronError::ApkParse("Invalid XAPK file".into()));
        }

        // Parse XAPK manifest
        let manifest = XapkParser::parse(xapk_path)?;

        // Create app directory
        let app_dir = PathBuf::from(&self.apps_dir).join(&manifest.package_name);
        std::fs::create_dir_all(&app_dir)?;

        // Extract all APKs
        let extracted_apks = XapkParser::extract_apks(xapk_path, &app_dir)?;

        // Extract OBBs if present
        let obb_dir = app_dir.join("obb");
        let _ = XapkParser::extract_obbs(xapk_path, &obb_dir);

        // Parse the base APK for native lib info
        let base_apk = extracted_apks.iter()
            .find(|p| p.contains("base") || !p.contains("config"))
            .or(extracted_apks.first())
            .ok_or_else(|| NeutronError::ApkParse("No base APK in XAPK".into()))?;

        let abi = if let Ok(meta) = ApkParser::parse(base_apk) {
            meta.abi
        } else {
            NativeAbi::Arm64V8a
        };

        // Extract native libs from all APKs
        let lib_dir = app_dir.join("lib");
        std::fs::create_dir_all(&lib_dir)?;
        for apk in &extracted_apks {
            let _ = ApkParser::extract_native_libs(apk, &lib_dir, abi);
        }

        // Initialize VFS
        let vfs = VfsOverlay::new(&manifest.package_name, &self.vfs_dir);
        vfs.initialize()?;

        let file_size = std::fs::metadata(xapk_path)?.len();

        let app = VirtualApp {
            id: self.next_id,
            package_name: manifest.package_name,
            label: manifest.name,
            apk_path: base_apk.clone(),
            abi,
            version_code: manifest.version_code,
            version_name: manifest.version_name,
            is_running: false,
            pid: 0,
            installed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            size_bytes: file_size,
            split_apks: extracted_apks,
            gg_compat,
        };

        self.next_id += 1;
        info!("XAPK installed: {} ({})", app.label, app.package_name);
        Ok(app)
    }

    /// Clone an app that's already installed on the system.
    pub fn clone_installed_app(&mut self, package_name: &str, gg_compat: bool) -> NeutronResult<VirtualApp> {
        info!("Cloning installed app: {}", package_name);

        // On Android, installed APKs are at /data/app/<pkg>-<hash>/base.apk
        // We try common paths
        let possible_paths = [
            format!("/data/app/{}/base.apk", package_name),
            format!("/data/app/~~*/{}/base.apk", package_name),
        ];

        // Use pm path equivalent — read from /proc/self/fd or try known locations
        // In practice on non-root, we'd use the PackageManager API via JNI,
        // but since we're pure Rust, we rely on the content resolver or known paths
        
        // Try to find the APK via /proc listing
        let apk_path = self.find_installed_apk(package_name)?;
        
        // Install from the found path
        self.install_apk(&apk_path, gg_compat)
    }

    /// Uninstall a virtual app.
    pub fn uninstall(&self, package_name: &str) -> NeutronResult<()> {
        let app_dir = PathBuf::from(&self.apps_dir).join(package_name);
        if app_dir.exists() {
            std::fs::remove_dir_all(&app_dir)?;
        }

        let vfs_dir = PathBuf::from(&self.vfs_dir).join(package_name);
        if vfs_dir.exists() {
            std::fs::remove_dir_all(&vfs_dir)?;
        }

        info!("Uninstalled: {}", package_name);
        Ok(())
    }

    /// List all installed virtual apps.
    pub fn list_installed(&self) -> NeutronResult<Vec<VirtualApp>> {
        let apps_dir = Path::new(&self.apps_dir);
        if !apps_dir.exists() {
            return Ok(Vec::new());
        }

        let mut apps = Vec::new();
        for entry in std::fs::read_dir(apps_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let meta_path = entry.path().join("app.json");
                if meta_path.exists() {
                    if let Ok(data) = std::fs::read_to_string(&meta_path) {
                        if let Ok(app) = serde_json::from_str::<VirtualApp>(&data) {
                            apps.push(app);
                        }
                    }
                }
            }
        }

        Ok(apps)
    }

    // --- Private ---

    fn find_installed_apk(&self, package_name: &str) -> NeutronResult<String> {
        // Try to read from /proc/self/fd symlinks or known APK locations
        let base_path = format!("/data/app/{}", package_name);
        
        // Walk /data/app looking for matching package
        if let Ok(entries) = std::fs::read_dir("/data/app") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(package_name) {
                    let apk_path = entry.path().join("base.apk");
                    if apk_path.exists() {
                        return Ok(apk_path.to_string_lossy().to_string());
                    }
                }
            }
        }

        Err(NeutronError::ApkParse(
            format!("Could not find installed APK for: {}", package_name)
        ))
    }
}
