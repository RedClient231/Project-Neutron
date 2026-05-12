//! # Neutron App — Android Entry Point
//!
//! Pure Rust Android app using NativeActivity + egui/eframe. No Java/Kotlin code.

use log::info;
use neutron_core::{NeutronConfig, ImportSource};
use neutron_engine::AppLauncher;
use neutron_apk::ApkInstaller;
use std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};

/// App state for the egui UI.
struct NeutronSpaceApp {
    /// Current view
    view: AppView,
    /// Status message
    status: String,
    /// File browser current directory
    current_dir: String,
    /// Files in the current directory
    files: Vec<FileBrowserEntry>,
    /// Installed apps list
    installed_apps: Vec<InstalledAppEntry>,
    /// APK installer
    installer: Arc<Mutex<ApkInstaller>>,
    /// App launcher
    launcher: Arc<Mutex<AppLauncher>>,
    /// Whether an import is in progress
    importing: bool,
}

#[derive(Clone, PartialEq)]
enum AppView {
    Main,
    FileBrowser,
}

#[derive(Clone)]
struct FileBrowserEntry {
    name: String,
    path: String,
    is_dir: bool,
    is_installable: bool,
    size_mb: f64,
}

#[derive(Clone)]
struct InstalledAppEntry {
    id: u64,
    name: String,
    package_name: String,
    version: String,
    size_mb: f64,
    is_running: bool,
    gg_compat: bool,
}

impl NeutronSpaceApp {
    fn new(installer: Arc<Mutex<ApkInstaller>>, launcher: Arc<Mutex<AppLauncher>>) -> Self {
        Self {
            view: AppView::Main,
            status: "Ready — Tap Import to add APK/XAPK files".into(),
            current_dir: "/storage/emulated/0".into(),
            files: Vec::new(),
            installed_apps: Vec::new(),
            installer,
            launcher,
            importing: false,
        }
    }

    /// Scan a directory for folders and APK/XAPK files.
    fn scan_directory(&mut self) {
        self.files.clear();
        let dir = &self.current_dir;
        info!("Scanning: {}", dir);

        match std::fs::read_dir(dir) {
            Ok(entries) => {
                let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                items.sort_by(|a, b| {
                    let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    match (a_dir, b_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.file_name().cmp(&b.file_name()),
                    }
                });

                for entry in items {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    let full_path = entry.path().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                    if is_dir {
                        self.files.push(FileBrowserEntry {
                            name,
                            path: full_path,
                            is_dir: true,
                            is_installable: false,
                            size_mb: 0.0,
                        });
                    } else {
                        // Show ALL files — mark APK/XAPK as installable
                        let lower = name.to_lowercase();
                        let is_installable = lower.ends_with(".apk")
                            || lower.ends_with(".xapk")
                            || lower.ends_with(".apks");
                        let size = entry.metadata().map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
                        self.files.push(FileBrowserEntry {
                            name,
                            path: full_path,
                            is_dir: false,
                            is_installable,
                            size_mb: size,
                        });
                    }
                }

                if self.files.is_empty() {
                    self.status = "No APK/XAPK files found in this folder".into();
                } else {
                    self.status = format!("Found {} items", self.files.len());
                }
            }
            Err(e) => {
                self.status = format!("⚠ Cannot read folder: {}", e);
                self.files.clear();
                info!("read_dir failed for {}: {}", dir, e);
            }
        }
    }

    /// Navigate up one directory level.
    fn go_up(&mut self) {
        if let Some(parent) = Path::new(&self.current_dir).parent() {
            self.current_dir = parent.to_string_lossy().to_string();
        }
        self.scan_directory();
    }

    /// Navigate into a directory.
    fn enter_dir(&mut self, path: &str) {
        self.current_dir = path.to_string();
        self.scan_directory();
    }

    /// Install an APK/XAPK file.
    fn install_file(&mut self, path: &str) {
        let filename = Path::new(path).file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());

        self.status = format!("Installing {}...", filename);
        self.importing = true;

        let source = ImportSource::FilePath(path.to_string());
        match self.installer.lock() {
            Ok(mut inst) => {
                match inst.install(source, true) {
                    Ok(app) => {
                        self.status = format!("✓ Installed: {}", app.label);
                        self.installed_apps.push(InstalledAppEntry {
                            id: app.id,
                            name: app.label,
                            package_name: app.package_name,
                            version: app.version_name,
                            size_mb: app.size_bytes as f64 / (1024.0 * 1024.0),
                            is_running: false,
                            gg_compat: app.gg_compat,
                        });
                        self.view = AppView::Main;
                    }
                    Err(e) => {
                        self.status = format!("✗ Failed: {}", e);
                    }
                }
            }
            Err(e) => {
                self.status = format!("Error: {}", e);
            }
        }
        self.importing = false;
    }
}

impl eframe::App for NeutronSpaceApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Dark theme
        ctx.set_visuals(egui::Visuals::dark());

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(26, 26, 46)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

                // Header
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("Neutron Space").color(egui::Color32::from_rgb(233, 69, 96)).size(24.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("v1.0").color(egui::Color32::GRAY).size(12.0));
                    });
                });

                ui.separator();

                // Status bar
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(22, 33, 62))
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&self.status).color(egui::Color32::from_rgb(170, 170, 170)).size(12.0));
                    });

                ui.add_space(4.0);

                match self.view.clone() {
                    AppView::Main => self.draw_main_view(ui),
                    AppView::FileBrowser => self.draw_file_browser(ui),
                }

                // Footer
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("Neutron Virtual Space — No Root Required").color(egui::Color32::from_rgb(68, 68, 68)).size(10.0));
                });
            });
    }
}

impl NeutronSpaceApp {
    fn draw_main_view(&mut self, ui: &mut egui::Ui) {
        // Import button
        let btn = ui.add_sized(
            [ui.available_width(), 44.0],
            egui::Button::new(
                egui::RichText::new(if self.importing { "Importing..." } else { "📦  Import APK / XAPK" })
                    .size(16.0)
            )
        );
        if btn.clicked() && !self.importing {
            self.view = AppView::FileBrowser;
            self.current_dir = "/storage/emulated/0/Download".into();
            self.scan_directory();
            // If Download doesn't work, try root
            if self.files.is_empty() && self.status.contains("Cannot read") {
                self.current_dir = "/storage/emulated/0".into();
                self.scan_directory();
            }
        }

        ui.add_space(8.0);

        // Installed apps header
        ui.label(egui::RichText::new(format!("Installed Apps ({})", self.installed_apps.len()))
            .color(egui::Color32::from_rgb(200, 200, 200))
            .size(14.0));

        ui.add_space(4.0);

        if self.installed_apps.is_empty() {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(22, 33, 62))
                .rounding(8.0)
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("No apps installed yet").color(egui::Color32::GRAY).size(14.0));
                        ui.label(egui::RichText::new("Tap Import to add APK/XAPK files").color(egui::Color32::DARK_GRAY).size(12.0));
                    });
                });
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let apps = self.installed_apps.clone();
                for app in &apps {
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(22, 33, 62))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Icon
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                                ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(233, 69, 96));
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, &app.name[..1.min(app.name.len())], egui::FontId::proportional(16.0), egui::Color32::WHITE);

                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&app.name).color(egui::Color32::WHITE).size(14.0));
                                    ui.label(egui::RichText::new(&app.package_name).color(egui::Color32::GRAY).size(11.0));
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(format!("v{}", app.version)).color(egui::Color32::DARK_GRAY).size(10.0));
                                        ui.label(egui::RichText::new(format!("{:.1} MB", app.size_mb)).color(egui::Color32::DARK_GRAY).size(10.0));
                                        if app.gg_compat {
                                            ui.label(egui::RichText::new("GG").color(egui::Color32::from_rgb(255, 152, 0)).size(10.0));
                                        }
                                    });
                                });
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        }
    }

    fn draw_file_browser(&mut self, ui: &mut egui::Ui) {
        // Header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Select APK / XAPK").color(egui::Color32::WHITE).size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    self.view = AppView::Main;
                    self.status = "Ready".into();
                }
            });
        });

        // Current path
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(15, 52, 96))
            .rounding(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&self.current_dir).color(egui::Color32::from_rgb(142, 197, 252)).size(11.0));
            });

        // Go up button
        if ui.add_sized([ui.available_width(), 36.0], egui::Button::new("⬆  Go Up (..)")).clicked() {
            self.go_up();
        }

        ui.add_space(4.0);

        // Permission error hint
        if self.status.contains("Cannot read") {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(80, 20, 20))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("⚠ Storage permission required!").color(egui::Color32::from_rgb(255, 100, 100)).size(13.0));
                    ui.label(egui::RichText::new("Go to: Settings → Apps → Neutron Space → Permissions → Storage → Allow all").color(egui::Color32::from_rgb(200, 200, 200)).size(11.0));
                });
        }

        // File list with scrolling
        let files_clone = self.files.clone();
        let mut action: Option<(bool, String)> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for file in &files_clone {
                let bg = if file.is_dir {
                    egui::Color32::from_rgb(31, 41, 55)
                } else {
                    egui::Color32::from_rgb(22, 33, 62)
                };

                egui::Frame::none()
                    .fill(bg)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Icon
                            let icon_color = if file.is_dir {
                                egui::Color32::from_rgb(59, 130, 246)
                            } else if file.is_installable {
                                egui::Color32::from_rgb(16, 185, 129)
                            } else {
                                egui::Color32::from_rgb(100, 100, 100)
                            };
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 6.0, icon_color);
                            let icon_text = if file.is_dir { "📁" } else if file.is_installable { "📦" } else { "📄" };
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon_text, egui::FontId::proportional(14.0), egui::Color32::WHITE);

                            // Name + info
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&file.name).color(egui::Color32::WHITE).size(13.0));
                                if file.is_dir {
                                    ui.label(egui::RichText::new("Folder").color(egui::Color32::GRAY).size(10.0));
                                } else {
                                    ui.label(egui::RichText::new(format!("{:.1} MB", file.size_mb)).color(egui::Color32::GRAY).size(10.0));
                                }
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if file.is_dir {
                                    if ui.button("Open").clicked() {
                                        action = Some((true, file.path.clone()));
                                    }
                                } else if file.is_installable {
                                    if ui.button("Install").clicked() {
                                        action = Some((false, file.path.clone()));
                                    }
                                }
                                // Non-installable files have no button — they're just visible
                            });
                        });
                    });

                ui.add_space(2.0);
            }
        });

        // Process action after drawing (avoids borrow issues)
        if let Some((is_dir, path)) = action {
            if is_dir {
                self.enter_dir(&path);
            } else {
                self.install_file(&path);
            }
        }
    }
}

/// Android entry point
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("NeutronSpace"),
    );

    info!("Neutron Space v1.0 starting with egui...");

    let data_dir = app
        .internal_data_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/data/data/com.neutron.virtualspace/files".into());

    let config = NeutronConfig::load_or_default(&data_dir);
    let launcher = Arc::new(Mutex::new(AppLauncher::new(config.clone())));
    let installer = Arc::new(Mutex::new(
        ApkInstaller::new(&config.apps_dir, &config.vfs_dir)
    ));

    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };

    eframe::run_native(
        "Neutron Space",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(NeutronSpaceApp::new(installer, launcher)))
        }),
    ).unwrap();
}
