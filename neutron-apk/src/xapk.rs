//! XAPK bundle parser.
//!
//! XAPK files are ZIP archives containing:
//! - manifest.json (package metadata)
//! - One or more APK files (base + splits)
//! - Optional OBB files

use neutron_core::{NativeAbi, NeutronError, NeutronResult};
use serde::{Deserialize, Serialize};
use log::{debug, info};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// XAPK manifest.json structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XapkManifest {
    pub package_name: String,
    pub name: String,
    pub version_code: u64,
    pub version_name: String,
    pub min_sdk_version: u32,
    pub target_sdk_version: u32,
    #[serde(default)]
    pub split_apks: Vec<XapkSplitInfo>,
    #[serde(default)]
    pub expansions: Vec<XapkExpansion>,
    #[serde(default)]
    pub total_size: u64,
}

/// Split APK info within an XAPK.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XapkSplitInfo {
    pub file: String,
    #[serde(default)]
    pub id: String,
}

/// OBB/expansion file info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XapkExpansion {
    pub file: String,
    #[serde(default)]
    pub install_path: String,
}

/// XAPK parser for split APK bundles.
pub struct XapkParser;

impl XapkParser {
    /// Parse an XAPK file and extract its manifest.
    pub fn parse<P: AsRef<Path>>(xapk_path: P) -> NeutronResult<XapkManifest> {
        let file = std::fs::File::open(xapk_path.as_ref())?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid XAPK zip: {}", e)))?;

        // Read manifest.json
        let mut manifest_entry = archive.by_name("manifest.json")
            .map_err(|_| NeutronError::ApkParse("No manifest.json in XAPK".into()))?;

        let mut manifest_json = String::new();
        manifest_entry.read_to_string(&mut manifest_json)?;

        let manifest: XapkManifest = serde_json::from_str(&manifest_json)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid manifest.json: {}", e)))?;

        info!("Parsed XAPK: {} v{}", manifest.package_name, manifest.version_name);
        Ok(manifest)
    }

    /// Extract all APKs from the XAPK bundle to the target directory.
    pub fn extract_apks<P: AsRef<Path>>(
        xapk_path: P,
        target_dir: P,
    ) -> NeutronResult<Vec<String>> {
        let file = std::fs::File::open(xapk_path.as_ref())?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid XAPK: {}", e)))?;

        let out_dir = target_dir.as_ref();
        std::fs::create_dir_all(out_dir)?;

        let mut extracted = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| NeutronError::ApkParse(e.to_string()))?;
            
            let name = entry.name().to_string();
            if name.ends_with(".apk") {
                let out_path = out_dir.join(&name);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                
                let mut out_file = std::fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut out_file)?;
                extracted.push(out_path.to_string_lossy().to_string());
                debug!("Extracted APK: {}", name);
            }
        }

        info!("Extracted {} APKs from XAPK", extracted.len());
        Ok(extracted)
    }

    /// Extract OBB files from the XAPK to the target directory.
    pub fn extract_obbs<P: AsRef<Path>>(
        xapk_path: P,
        target_dir: P,
    ) -> NeutronResult<Vec<String>> {
        let file = std::fs::File::open(xapk_path.as_ref())?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid XAPK: {}", e)))?;

        let out_dir = target_dir.as_ref();
        std::fs::create_dir_all(out_dir)?;

        let mut extracted = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| NeutronError::ApkParse(e.to_string()))?;
            
            let name = entry.name().to_string();
            if name.ends_with(".obb") {
                let out_path = out_dir.join(&name);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                
                let mut out_file = std::fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut out_file)?;
                extracted.push(out_path.to_string_lossy().to_string());
                debug!("Extracted OBB: {}", name);
            }
        }

        Ok(extracted)
    }

    /// Validate an XAPK file.
    pub fn validate<P: AsRef<Path>>(xapk_path: P) -> NeutronResult<bool> {
        let file = std::fs::File::open(xapk_path.as_ref())?;
        let archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid zip: {}", e)))?;

        // Must contain manifest.json and at least one .apk
        let has_manifest = (0..archive.len()).any(|i| {
            archive.name_for_index(i)
                .map(|n| n == "manifest.json")
                .unwrap_or(false)
        });

        let has_apk = (0..archive.len()).any(|i| {
            archive.name_for_index(i)
                .map(|n| n.ends_with(".apk"))
                .unwrap_or(false)
        });

        Ok(has_manifest && has_apk)
    }
}
