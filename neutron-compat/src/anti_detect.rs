//! Anti-detection — prevents games from detecting the virtual environment.
//!
//! Many games use anti-cheat that detects:
//! - Running inside a virtual app (VirtualApp, Parallel Space)
//! - Xposed/Frida/Magisk
//! - Rooted devices
//! - Debuggers attached
//! - Modified /proc entries
//!
//! This module spoofs all detection vectors.

use neutron_core::{NeutronError, NeutronResult};
use log::debug;
use std::collections::HashSet;

/// Anti-detection evasion for virtual environment.
pub struct AntiDetection {
    /// Packages to hide from package list queries
    hidden_packages: HashSet<String>,
    /// Files to hide (return ENOENT)
    hidden_files: Vec<String>,
    /// Properties to spoof
    spoofed_props: Vec<(String, String)>,
}

impl AntiDetection {
    /// Create a new anti-detection handler.
    pub fn new() -> Self {
        let mut ad = Self {
            hidden_packages: HashSet::new(),
            hidden_files: Vec::new(),
            spoofed_props: Vec::new(),
        };
        ad.setup_defaults();
        ad
    }

    /// Check if a file path should be hidden (ENOENT).
    pub fn should_hide_file(&self, path: &str) -> bool {
        self.hidden_files.iter().any(|h| path.contains(h))
    }

    /// Check if a package should be hidden from queries.
    pub fn should_hide_package(&self, package: &str) -> bool {
        self.hidden_packages.contains(package)
    }

    /// Get spoofed value for a system property.
    pub fn spoof_property(&self, name: &str) -> Option<&str> {
        self.spoofed_props.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Generate a clean /proc/self/status that hides TracerPid.
    pub fn clean_status(raw_status: &str) -> String {
        raw_status.lines()
            .map(|line| {
                if line.starts_with("TracerPid:") {
                    "TracerPid:\t0"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Generate a clean /proc/self/maps that removes traces.
    pub fn clean_maps(raw_maps: &str) -> String {
        raw_maps.lines()
            .filter(|line| {
                let lower = line.to_lowercase();
                !lower.contains("neutron")
                    && !lower.contains("frida")
                    && !lower.contains("magisk")
                    && !lower.contains("xposed")
                    && !lower.contains("substrate")
                    && !lower.contains("gameguardian")
                    && !lower.contains("lspd")
                    && !lower.contains("edxposed")
                    && !lower.contains("lsposed")
                    && !lower.contains("riru")
                    && !lower.contains("zygisk")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// List of paths that should always return ENOENT to prevent root detection.
    pub fn root_detection_paths() -> &'static [&'static str] {
        &[
            "/system/app/Superuser.apk",
            "/system/app/SuperSU",
            "/system/xbin/su",
            "/system/bin/su",
            "/sbin/su",
            "/su/bin/su",
            "/data/local/su",
            "/data/local/bin/su",
            "/data/local/xbin/su",
            "/system/bin/.ext",
            "/system/usr/we-need-root",
            "/system/etc/init.d",
            "/system/bin/failsafe/su",
            "/data/adb/magisk",
            "/data/adb/modules",
            "/system/bin/magisk",
            "/sbin/.magisk",
            "/cache/.disable_magisk",
            "/dev/.magisk.unblock",
        ]
    }

    /// Packages commonly detected by anti-cheat.
    pub fn suspicious_packages() -> &'static [&'static str] {
        &[
            "com.topjohnwu.magisk",
            "de.robv.android.xposed.installer",
            "com.saurik.substrate",
            "org.lsposed.manager",
            "io.github.lsposed.manager",
            "com.tsng.hidemyapplist",
            "com.noshufou.android.su",
            "eu.chainfire.supersu",
            "com.koushikdutta.superuser",
            "com.thirdparty.superuser",
            "com.yellowes.su",
            "me.phh.superuser",
            "com.kingouser.com",
            "com.android.vending.billing.InAppBillingService.LUCK",
        ]
    }

    // --- Private ---

    fn setup_defaults(&mut self) {
        // Hide root detection files
        for path in Self::root_detection_paths() {
            self.hidden_files.push(path.to_string());
        }

        // Hide virtual environment traces
        self.hidden_files.push("com.neutron.virtualspace".into());
        self.hidden_files.push("neutron".into());

        // Hide suspicious packages
        for pkg in Self::suspicious_packages() {
            self.hidden_packages.insert(pkg.to_string());
        }
        // Also hide ourselves
        self.hidden_packages.insert("com.neutron.virtualspace".into());

        // Property spoofing
        self.spoofed_props.push(("ro.debuggable".into(), "0".into()));
        self.spoofed_props.push(("ro.secure".into(), "1".into()));
        self.spoofed_props.push(("ro.build.tags".into(), "release-keys".into()));
        self.spoofed_props.push(("ro.build.type".into(), "user".into()));
    }
}

impl Default for AntiDetection {
    fn default() -> Self {
        Self::new()
    }
}
