//! Path Redirector — handles syscall-level path rewriting.
//!
//! When a virtualized process makes an openat/stat/readlink syscall,
//! this module intercepts the path argument and rewrites it to point
//! to the virtual filesystem location.

use neutron_core::{NeutronError, NeutronResult};
use log::debug;
use std::collections::HashMap;

/// Path redirector that rewrites filesystem paths for virtualized processes.
#[derive(Debug)]
pub struct PathRedirector {
    /// Map of original path prefixes to virtual targets
    prefix_map: HashMap<String, String>,
    /// Exact path replacements
    exact_map: HashMap<String, String>,
    /// Package name of the current virtual app
    package_name: String,
    /// Virtual storage root
    virtual_root: String,
}

impl PathRedirector {
    /// Create a new path redirector for the given virtual app.
    pub fn new(package_name: &str, virtual_root: &str) -> Self {
        let mut redirector = Self {
            prefix_map: HashMap::new(),
            exact_map: HashMap::new(),
            package_name: package_name.to_string(),
            virtual_root: virtual_root.to_string(),
        };
        redirector.setup_default_redirects();
        redirector
    }

    /// Redirect a path, returning the new path or None if no redirect needed.
    pub fn redirect(&self, path: &str) -> Option<String> {
        // Check exact matches first
        if let Some(target) = self.exact_map.get(path) {
            return Some(target.clone());
        }

        // Check prefix matches
        for (prefix, target) in &self.prefix_map {
            if path.starts_with(prefix) {
                let suffix = &path[prefix.len()..];
                return Some(format!("{}{}", target, suffix));
            }
        }

        None
    }

    /// Check if a path should be completely hidden (return ENOENT).
    pub fn should_hide(&self, path: &str) -> bool {
        let hidden_patterns = [
            "/data/data/com.neutron.virtualspace",
            "/proc/self/status",  // Will be spoofed separately
            "magisk",
            "frida",
            "xposed",
        ];

        hidden_patterns.iter().any(|p| path.contains(p))
    }

    /// Add a custom redirect rule.
    pub fn add_prefix_redirect(&mut self, from: &str, to: &str) {
        self.prefix_map.insert(from.to_string(), to.to_string());
    }

    /// Add an exact path redirect.
    pub fn add_exact_redirect(&mut self, from: &str, to: &str) {
        self.exact_map.insert(from.to_string(), to.to_string());
    }

    /// Rewrite a path buffer in-place (for modifying ptrace'd syscall args).
    /// Returns the new null-terminated byte buffer to write into the tracee.
    pub fn rewrite_path_bytes(&self, original: &[u8]) -> Option<Vec<u8>> {
        // Convert bytes to string (paths are typically UTF-8 on Android)
        let path_str = std::str::from_utf8(original).ok()?;
        let path_trimmed = path_str.trim_end_matches('\0');
        
        if let Some(redirected) = self.redirect(path_trimmed) {
            debug!("Path redirect: {} -> {}", path_trimmed, redirected);
            let mut bytes = redirected.into_bytes();
            bytes.push(0); // null terminator
            Some(bytes)
        } else {
            None
        }
    }

    // --- Private ---

    fn setup_default_redirects(&mut self) {
        let pkg = &self.package_name.clone();
        let root = &self.virtual_root.clone();

        // App data directories
        self.prefix_map.insert(
            format!("/data/data/{}", pkg),
            format!("{}/data", root),
        );
        self.prefix_map.insert(
            format!("/data/user/0/{}", pkg),
            format!("{}/data", root),
        );

        // External storage
        self.prefix_map.insert(
            format!("/storage/emulated/0/Android/data/{}", pkg),
            format!("{}/external_data", root),
        );
        self.prefix_map.insert(
            format!("/storage/emulated/0/Android/obb/{}", pkg),
            format!("{}/obb", root),
        );

        // Native library path
        self.prefix_map.insert(
            format!("/data/app/~~nonce~~/{}-nonce/lib/", pkg),
            format!("{}/lib/", root),
        );

        // Cache
        self.prefix_map.insert(
            format!("/data/data/{}/cache", pkg),
            format!("{}/cache", root),
        );
    }
}
