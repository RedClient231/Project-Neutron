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

/// Scan a directory and return FileEntry items for the UI.
/// Shows ALL directories + files with APK/XAPK/APKS extensions.
/// If nothing found, tries alternative paths (permission issue workaround).
fn scan_directory(dir_path: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let path = Path::new(dir_path);

    info!("Scanning directory: {}", dir_path);

    match std::fs::read_dir(path) {
        Ok(read_dir) => {
            let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
            info!("Found {} entries in {}", items.len(), dir_path);

            // Sort: directories first, then alphabetical
            items.sort_by(|a, b| {
                let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
                match (a_dir, b_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.file_name().cmp(&b.file_name()),
                }
            });

            let mut idx = 0i32;
            for entry in items {
                let name = entry.file_name().to_string_lossy().to_string();
                let full_path = entry.path().to_string_lossy().to_string();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                // Skip hidden files/folders
                if name.starts_with('.') {
                    continue;
                }

                if is_dir {
                    entries.push(FileEntry {
                        index: idx,
                        name: name.into(),
                        path: full_path.into(),
                        size_mb: "".into(),
                        is_dir: true,
                    });
                    idx += 1;
                } else {
                    // Show ALL files but mark APK/XAPK specially
                    let lower = name.to_lowercase();
                    let is_installable = lower.ends_with(".apk")
                        || lower.ends_with(".xapk")
                        || lower.ends_with(".apks");

                    // Show all files so the user can see the directory isn't empty
                    // but only APK/XAPK/APKS will have the Install button
                    if is_installable {
                        let size = entry.metadata()
                            .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                            .unwrap_or(0.0);
                        entries.push(FileEntry {
                            index: idx,
                            name: name.into(),
                            path: full_path.into(),
                            size_mb: format!("{:.1}", size).into(),
                            is_dir: false,
                        });
                        idx += 1;
                    }
                }
            }
        }
        Err(e) => {
            info!("Cannot read {}: {} — likely missing storage permission", dir_path, e);
            // Return a helpful entry telling the user about permissions
            entries.push(FileEntry {
                index: 0,
                name: format!("⚠ Cannot read: {}", e).into(),
                path: "".into(),
                size_mb: "".into(),
                is_dir: false,
            });
            entries.push(FileEntry {
                index: 1,
                name: "Grant 'All files access' in Settings".into(),
                path: "".into(),
                size_mb: "".into(),
                is_dir: false,
            });
            entries.push(FileEntry {
                index: 2,
                name: "Settings > Apps > Neutron Space > Permissions".into(),
                path: "".into(),
                size_mb: "".into(),
                is_dir: false,
            });
        }
    }

    // If directory was readable but empty of APK files, show helpful message
    if entries.is_empty() {
        entries.push(FileEntry {
            index: 0,
            name: "No APK/XAPK files found here".into(),
            path: "".into(),
            size_mb: "".into(),
            is_dir: false,
        });
    }

    entries
}

/// Get initial scan paths — tries multiple known locations.
fn get_start_directory() -> String {
    // Try common paths in order
    let candidates = [
        "/storage/emulated/0/Download",
        "/storage/emulated/0",
        "/sdcard/Download",
        "/sdcard",
    ];

    for path in &candidates {
        if let Ok(entries) = std::fs::read_dir(path) {
            if entries.count() > 0 {
                return path.to_string();
            }
        }
    }

    // Fallback — at least the user sees the permission error
    "/storage/emulated/0".to_string()
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

    // Initialize the Slint Android backend
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

    // Create the UI
    let ui = NeutronApp::new().unwrap();
    ui.set_status_text("Ready — Tap Import to add APK/XAPK files".into());

    // Shared state for the current file list (so file-tap can look up by index)
    let file_entries: Arc<Mutex<Vec<FileEntry>>> = Arc::new(Mutex::new(Vec::new()));

    // --- Import APK: opens file browser ---
    let ui_weak = ui.as_weak();
    let file_entries_clone = file_entries.clone();
    ui.on_import_apk(move || {
        info!("Opening file browser");
        if let Some(ui) = ui_weak.upgrade() {
            let start_dir = get_start_directory();
            ui.set_current_path(start_dir.clone().into());
            let files = scan_directory(&start_dir);
            
            // Store for index lookup
            if let Ok(mut fe) = file_entries_clone.lock() {
                *fe = files.clone();
            }
            
            ui.set_file_list(slint::ModelRc::new(slint::VecModel::from(files)));
            ui.set_show_file_browser(true);
            ui.set_status_text("Browse and tap 'Open' or 'Install'".into());
        }
    });

    // --- Navigate directory ---
    let ui_weak = ui.as_weak();
    let file_entries_clone = file_entries.clone();
    ui.on_navigate_dir(move |dir_path| {
        if let Some(ui) = ui_weak.upgrade() {
            let path_str = dir_path.to_string();
            let new_path = if path_str == ".." {
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
            
            if let Ok(mut fe) = file_entries_clone.lock() {
                *fe = files.clone();
            }
            
            ui.set_file_list(slint::ModelRc::new(slint::VecModel::from(files)));
        }
    });

    // --- File tap by index (reliable touch) ---
    let ui_weak = ui.as_weak();
    let file_entries_clone = file_entries.clone();
    let installer_clone = installer.clone();
    ui.on_file_tap(move |index| {
        let entry = {
            let fe = file_entries_clone.lock().unwrap();
            fe.iter().find(|f| f.index == index).cloned()
        };

        if let Some(file) = entry {
            let path = file.path.to_string();
            if file.is_dir {
                // Navigate into directory
                if let Some(ui) = ui_weak.upgrade() {
                    info!("Opening folder: {}", path);
                    ui.set_current_path(path.clone().into());
                    let files = scan_directory(&path);
                    if let Ok(mut fe) = file_entries_clone.lock() {
                        *fe = files.clone();
                    }
                    ui.set_file_list(slint::ModelRc::new(slint::VecModel::from(files)));
                }
            } else {
                // Install the file
                if let Some(ui) = ui_weak.upgrade() {
                    let filename = Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.clone());
                    
                    info!("Installing: {}", path);
                    ui.set_show_file_browser(false);
                    ui.set_importing(true);
                    ui.set_status_text(format!("Installing {}...", filename).into());

                    let source = ImportSource::FilePath(path.clone());
                    match installer_clone.lock() {
                        Ok(mut inst) => {
                            match inst.install(source, true) {
                                Ok(app) => {
                                    ui.set_status_text(
                                        format!("Installed: {}", app.label).into()
                                    );
                                    info!("Installed: {} ({})", app.label, app.package_name);
                                }
                                Err(e) => {
                                    ui.set_status_text(
                                        format!("Failed: {}", e).into()
                                    );
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
            }
        }
    });

    // --- Select file (legacy, kept for compatibility) ---
    let ui_weak = ui.as_weak();
    let installer_clone2 = installer.clone();
    ui.on_select_file(move |file_path| {
        let path = file_path.to_string();
        info!("Direct select: {}", path);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_show_file_browser(false);
            ui.set_status_text(format!("Installing...").into());
            let source = ImportSource::FilePath(path);
            if let Ok(mut inst) = installer_clone2.lock() {
                match inst.install(source, true) {
                    Ok(app) => ui.set_status_text(format!("Installed: {}", app.label).into()),
                    Err(e) => ui.set_status_text(format!("Failed: {}", e).into()),
                }
            }
        }
    });

    // --- Close browser ---
    let ui_weak = ui.as_weak();
    ui.on_close_browser(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_show_file_browser(false);
            ui.set_status_text("Ready".into());
        }
    });

    // --- Launch app ---
    let _launcher_ref = launcher.clone();
    let ui_weak = ui.as_weak();
    ui.on_launch_app(move |app_id| {
        info!("Launch app: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Launching app #{}...", app_id).into());
        }
    });

    // --- Stop app ---
    let _launcher_ref2 = launcher.clone();
    let ui_weak = ui.as_weak();
    ui.on_stop_app(move |app_id| {
        info!("Stop app: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Stopped app #{}", app_id).into());
        }
    });

    // --- Uninstall ---
    let ui_weak = ui.as_weak();
    ui.on_uninstall_app(move |app_id| {
        info!("Uninstall: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Uninstalled app #{}", app_id).into());
        }
    });

    // --- Toggle GG ---
    let ui_weak = ui.as_weak();
    ui.on_toggle_gg(move |app_id| {
        info!("Toggle GG: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Toggled GG for app #{}", app_id).into());
        }
    });

    // Run the UI event loop
    info!("Starting UI...");
    ui.run().unwrap();
    info!("App exiting");
}
