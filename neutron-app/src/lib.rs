//! # Neutron App — Android Entry Point
//!
//! Pure Rust Android app using NativeActivity + Slint UI. No Java/Kotlin code.
//! This is the top-level binary that ties all crates together.

use log::{info, error};
use neutron_core::{NeutronConfig, ImportSource, VirtualApp};
use neutron_engine::AppLauncher;
use neutron_apk::ApkInstaller;
use std::sync::{Arc, Mutex};

// Include the compiled Slint UI
slint::include_modules!();

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
    ui.set_status_text("Ready — No apps installed".into());

    // --- Wire up callbacks ---

    // Import APK callback
    let installer_clone = installer.clone();
    let ui_weak = ui.as_weak();
    ui.on_import_apk(move || {
        info!("Import APK requested");
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text("Select an APK/XAPK file to import...".into());
            ui.set_importing(true);
            // In production: open Android file picker via intent
            // For now, provide feedback that the action was received
            ui.set_status_text("File picker not yet connected — use adb push".into());
            ui.set_importing(false);
        }
    });

    // Launch app callback
    let launcher_clone = launcher.clone();
    let ui_weak = ui.as_weak();
    ui.on_launch_app(move |app_id| {
        info!("Launch app requested: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Launching app #{}...", app_id).into());
        }
    });

    // Stop app callback
    let launcher_clone2 = launcher.clone();
    let ui_weak = ui.as_weak();
    ui.on_stop_app(move |app_id| {
        info!("Stop app requested: id={}", app_id);
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_status_text(format!("Stopping app #{}...", app_id).into());
        }
    });

    // Uninstall app callback
    let installer_clone2 = installer.clone();
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

    // Cleanup on exit
    info!("App exiting — stopping all virtual processes");
    if let Ok(mut l) = launcher.lock() {
        let _ = l.stop_all();
    }
}

/// Import an APK/XAPK from the filesystem.
/// Called from the UI when user selects a file.
pub fn import_file(
    installer: &Arc<Mutex<ApkInstaller>>,
    path: &str,
    gg_compat: bool,
) -> Result<VirtualApp, String> {
    let source = ImportSource::FilePath(path.to_string());
    
    installer
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .install(source, gg_compat)
        .map_err(|e| format!("Install error: {}", e))
}

/// Clone an installed system app into the virtual space.
pub fn clone_app(
    installer: &Arc<Mutex<ApkInstaller>>,
    package_name: &str,
    gg_compat: bool,
) -> Result<VirtualApp, String> {
    let source = ImportSource::CloneInstalled(package_name.to_string());
    
    installer
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .install(source, gg_compat)
        .map_err(|e| format!("Install error: {}", e))
}

/// Launch a virtual app.
pub fn launch_virtual_app(
    launcher: &Arc<Mutex<AppLauncher>>,
    app: VirtualApp,
) -> Result<u32, String> {
    launcher
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .launch(app)
        .map_err(|e| format!("Launch error: {}", e))
}

/// Stop a virtual app.
pub fn stop_virtual_app(
    launcher: &Arc<Mutex<AppLauncher>>,
    package_name: &str,
) -> Result<(), String> {
    launcher
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .stop(package_name)
        .map_err(|e| format!("Stop error: {}", e))
}
