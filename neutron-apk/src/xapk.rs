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

/// Helper to deserialize a value that might be a number or a string containing a number.
fn deserialize_string_or_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct StringOrU64;
    impl<'de> de::Visitor<'de> for StringOrU64 {
        type Value = u64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or a string containing a number")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> { Ok(v) }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> { Ok(v as u64) }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u64, E> { Ok(v as u64) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.parse::<u64>().map_err(|_| de::Error::custom(format!("cannot parse '{}' as u64", v)))
        }
    }
    deserializer.deserialize_any(StringOrU64)
}

fn deserialize_string_or_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct StringOrU32;
    impl<'de> de::Visitor<'de> for StringOrU32 {
        type Value = u32;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or a string containing a number")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u32, E> { Ok(v as u32) }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u32, E> { Ok(v as u32) }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u32, E> { Ok(v as u32) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u32, E> {
            v.parse::<u32>().map_err(|_| de::Error::custom(format!("cannot parse '{}' as u32", v)))
        }
    }
    deserializer.deserialize_any(StringOrU32)
}

/// XAPK manifest.json structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XapkManifest {
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_string_or_u64")]
    pub version_code: u64,
    #[serde(default)]
    pub version_name: String,
    #[serde(default, deserialize_with = "deserialize_string_or_u32")]
    pub min_sdk_version: u32,
    #[serde(default, deserialize_with = "deserialize_string_or_u32")]
    pub target_sdk_version: u32,
    #[serde(default)]
    pub split_apks: Vec<XapkSplitInfo>,
    #[serde(default)]
    pub expansions: Vec<XapkExpansion>,
    #[serde(default, deserialize_with = "deserialize_string_or_u64")]
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
    pub fn extract_apks(
        xapk_path: &str,
        target_dir: &std::path::Path,
    ) -> NeutronResult<Vec<String>> {
        let file = std::fs::File::open(xapk_path)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid XAPK: {}", e)))?;

        std::fs::create_dir_all(target_dir)?;

        let mut extracted = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| NeutronError::ApkParse(e.to_string()))?;
            
            let name = entry.name().to_string();
            if name.ends_with(".apk") {
                let out_path = target_dir.join(&name);
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
    pub fn extract_obbs(
        xapk_path: &str,
        target_dir: &std::path::Path,
    ) -> NeutronResult<Vec<String>> {
        let file = std::fs::File::open(xapk_path)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid XAPK: {}", e)))?;

        std::fs::create_dir_all(target_dir)?;

        let mut extracted = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| NeutronError::ApkParse(e.to_string()))?;
            
            let name = entry.name().to_string();
            if name.ends_with(".obb") {
                let out_path = target_dir.join(&name);
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
