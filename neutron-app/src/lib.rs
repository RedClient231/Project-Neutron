//! # Neutron App — Android Entry Point
//!
//! Pure Rust Android app using NativeActivity. No Java/Kotlin code.
//! This is the top-level binary that ties all crates together.

use android_activity::{AndroidApp, MainEvent, PollEvent};
use log::{info, error, debug};
use neutron_core::{NeutronConfig, ImportSource, VirtualApp};
use neutron_engine::AppLauncher;
use neutron_apk::ApkInstaller;
use std::sync::{Arc, Mutex};

/// Main entry point for the Android NativeActivity.
#[no_mangle]
fn android_main(app: AndroidApp) {
    // Initialize logging
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("NeutronSpace"),
    );

    info!("Neutron Space v1.0 starting...");

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

    // Main event loop
    loop {
        app.poll_events(Some(std::time::Duration::from_millis(16)), |event| {
            match event {
                PollEvent::Main(main_event) => {
                    match main_event {
                        MainEvent::InitWindow { .. } => {
                            info!("Window initialized — starting UI");
                            // In production: Initialize Slint UI on the window
                        }
                        MainEvent::TerminateWindow { .. } => {
                            info!("Window terminated");
                        }
                        MainEvent::Destroy => {
                            info!("App destroying — stopping all virtual processes");
                            if let Ok(mut l) = launcher.lock() {
                                let _ = l.stop_all();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        });
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
