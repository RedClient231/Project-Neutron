//! # Neutron App — Android Entry Point
//!
//! Pure Rust Android app using NativeActivity + Slint UI. No Java/Kotlin code.
//! This is the top-level binary that ties all crates together.

use log::info;
use neutron_core::{NeutronConfig, ImportSource, VirtualApp};
use neutron_engine::AppLauncher;
use neutron_apk::ApkInstaller;
use std::sync::{Arc, Mutex};
use std::path::Path;

// Include the compiled Slint UI
slint::include_modules!();

/// Scan a directory for APK/XAPK files and subdirectories.
fn scan_directory(dir_path: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let path = Path::new(dir_path);

    if let Ok(read_dir) = std::fs::read_dir(path) {
        let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
        // Sort: directories first, then by name
        items.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        for entry in items {
            let name = entry.file_name().to_string_lossy().to_string();
            let full_path = entry.path().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            // Skip hidden files
            if name.starts_with('.') {
                continue;
            }

            if is_dir {
                entries.push(FileEntry {
                    name: name.into(),
                    path: full_path.into(),
                    size_mb: "".into(),
                    is_dir: true,
                });
            } else {
                // Only show APK and XAPK files
                let lower = name.to_lowercase();
                if lower.ends_with(".apk") || lower.ends_with(".xapk") || lower.ends_with(".apks") {
                    let size = entry.metadata()
                        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                        .unwrap_or(0.0);
                    entries.push(FileEntry {
                        name: name.into(),
                        path: full_path.into(),
                        size_mb: format!("{:.1}", size).into(),
                        is_dir: false,
                    });
                }
            }
        }
    }

    entries
}

/// Main entry point for the Android NativeActivity.
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    // Initialize logging
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("NeutronSpace"),
    );

    info!("Neutron Space v1.0 starting...");

    // Initialize the Slint Android backend with our AndroidApp handle
    slint::android::init(app.clone()).unwrap();

    // Get the app's internal data directory
    let data_dir = app
        .internal_data_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/data/data/com.neutron.virtualspace/files".into());

    // Initialize core systems
    let config = NeutronConfig::load_or_default(&data_dir);
    let launcher = Arc::new(Mutex::new(AppLauncher::new(config.clone())));
    let installer = Arc::new(Mutex::new(
        ApkInstaller::new(&config.apps_dir, &config.vfs_dir)
    ));

    info!("Neutron initialized. Data dir: {}", data_dir);

    // Create and show the Slint UI
    let ui = NeutronApp::new().unwrap();

    // Set initial status
    ui.set_status_text("Ready — Tap Import to add APK/XAPK files".into());

    // --- Wire up callbacks ---

    // Import APK — opens the built-in file browser
    let ui_weak = ui.as_weak();
    ui.on_import_apk(move || {
        info!("Opening file browser...");
        if let Some(ui) = ui_weak.upgrade() {
            let start_dir = "/storage/emulated/0";
            ui.set_current_path(start_dir.into());
            let files = scan_directory(start_dir);
            let model: Vec<FileEntry> = files;
            ui.set_file_list(slint::ModelRc::new(slint::VecModel::from(model)));
            ui.set_show_file_browser(true);
            ui.set_status_text("Browse for APK/XAPK files".into());
        }
    });

    // Navigate directory in file browser
    let ui_weak = ui.as_weak();
    ui.on_navigate_dir(move |dir_path| {
        if let Some(ui) = ui_weak.upgrade() {
            let path_str = dir_path.to_string();
            let new_path = if path_str == ".." {
                // Go up one level
                let current = ui.get_current_path().to_string();
                Path::new(&current)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/storage/emulated/0".to_string())
            } else {
                path_str
            };

            info!("Navigating to: {}", new_path);
            ui.set_current_path(new_path.clone().into());
            let files = scan_directory(&new_path);
            ui.set_file_list(slint::ModelRc::new(slint::VecModel::from(files)));
        }
    });

    // Select file from browser — install it
    let installer_clone = installer.clone();
    let ui_weak = ui.as_weak();
    ui.on_select_file(move |file_path| {
        let path = file_path.to_string();
        info!("Selected file for install: {}", path);

        if let Some(ui) = ui_weak.upgrade() {
            ui.set_show_file_browser(false);
            ui.set_importing(true);
            ui.set_status_text(format!("Installing {}...", 
                Path::new(&path).file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone())
            ).into());

            // Perform installation
            let source = ImportSource::FilePath(path.clone());
            match installer_clone.lock() {
                Ok(mut inst) => {
                    match inst.install(source, true) {
                        Ok(app) => {
                            ui.set_status_text(
                                format!("Installed: {} ({})", app.label, app.package_name).into()
                            );
                            info!("Successfully installed: {}", app.package_name);
                        }
                        Err(e) => {
                            ui.set_status_text(format!("Install failed: {}", e).into());
                            info!("Install failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    ui.set_status_text(format!("Error: {}", e).into());
                }
            }
            ui.set_importing(false);
        }
    });

    // Close file browser
    let ui_weak = ui.as_weak();
    ui.on_close_browser(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_show_file_browser(false);
            ui.set_status_text("Ready".into());
        }
    });

    // Launch app callback
    let _launcher_ref = launcher.clone();
    let ui_weak = ui.as_weak();
    ui.on_launch_app(move |app_id| {
        info!("Launch app requested: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Launching app #{}...", app_id).into());
        }
    });

    // Stop app callback
    let _launcher_ref2 = launcher.clone();
    let ui_weak = ui.as_weak();
    ui.on_stop_app(move |app_id| {
        info!("Stop app requested: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Stopping app #{}...", app_id).into());
        }
    });

    // Uninstall app callback
    let ui_weak = ui.as_weak();
    ui.on_uninstall_app(move |app_id| {
        info!("Uninstall app requested: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Uninstalled app #{}", app_id).into());
        }
    });

    // Toggle GameGuardian compat callback
    let ui_weak = ui.as_weak();
    ui.on_toggle_gg(move |app_id| {
        info!("Toggle GG compat for app: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Toggled GG compat for app #{}", app_id).into());
        }
    });

    // Run the Slint event loop (this blocks until the app is closed)
    info!("Starting Slint UI event loop...");
    ui.run().unwrap();

    info!("App exiting");
}
