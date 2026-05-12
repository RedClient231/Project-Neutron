//! APK file parser — extracts metadata and native libraries from APK archives.

use neutron_core::{NativeAbi, NeutronError, NeutronResult, VirtualApp};
use log::{debug, info, warn};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek};
use std::path::Path;
use zip::ZipArchive;

use crate::manifest::ManifestInfo;

/// Parsed APK metadata.
#[derive(Debug, Clone)]
pub struct ApkMetadata {
    pub package_name: String,
    pub version_code: u64,
    pub version_name: String,
    pub label: String,
    pub min_sdk: u32,
    pub target_sdk: u32,
    pub native_libs: Vec<NativeLibInfo>,
    pub abi: NativeAbi,
    pub total_size: u64,
    pub sha256: String,
    pub has_split_apks: bool,
}

/// Information about a native library within an APK.
#[derive(Debug, Clone)]
pub struct NativeLibInfo {
    pub name: String,
    pub abi: NativeAbi,
    pub path_in_apk: String,
    pub size: u64,
}

/// APK file parser.
pub struct ApkParser;

impl ApkParser {
    /// Parse an APK file and extract metadata.
    pub fn parse<P: AsRef<Path>>(apk_path: P) -> NeutronResult<ApkMetadata> {
        let path = apk_path.as_ref();
        let file = std::fs::File::open(path)?;
        let file_size = file.metadata()?.len();
        
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid APK zip: {}", e)))?;

        // Extract native libraries info
        let native_libs = Self::find_native_libs(&mut archive)?;
        let abi = Self::detect_abi(&native_libs);

        // Parse AndroidManifest.xml (binary XML)
        let manifest = Self::parse_manifest(&mut archive)?;

        // Calculate SHA256
        let sha256 = Self::compute_sha256(path)?;

        Ok(ApkMetadata {
            package_name: manifest.package_name,
            version_code: manifest.version_code,
            version_name: manifest.version_name,
            label: manifest.label,
            min_sdk: manifest.min_sdk,
            target_sdk: manifest.target_sdk,
            native_libs,
            abi,
            total_size: file_size,
            sha256,
            has_split_apks: false,
        })
    }

    /// Extract native libraries from the APK to the given directory.
    pub fn extract_native_libs<P: AsRef<Path>>(
        apk_path: P,
        target_dir: P,
        target_abi: NativeAbi,
    ) -> NeutronResult<Vec<String>> {
        let file = std::fs::File::open(apk_path.as_ref())?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid APK: {}", e)))?;

        let abi_dir = target_abi.lib_dir_name();
        let prefix = format!("lib/{}/", abi_dir);
        let out_dir = target_dir.as_ref();
        std::fs::create_dir_all(out_dir)?;

        let mut extracted = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| NeutronError::ApkParse(e.to_string()))?;
            
            let name = entry.name().to_string();
            if name.starts_with(&prefix) && name.ends_with(".so") {
                let lib_name = name.rsplit('/').next().unwrap_or(&name);
                let out_path = out_dir.join(lib_name);
                
                let mut out_file = std::fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut out_file)?;
                
                // Set executable permission
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&out_path, 
                        std::fs::Permissions::from_mode(0o755))?;
                }
                
                extracted.push(out_path.to_string_lossy().to_string());
                debug!("Extracted native lib: {}", lib_name);
            }
        }

        info!("Extracted {} native libraries for {}", extracted.len(), abi_dir);
        Ok(extracted)
    }

    /// Validate that an APK file is well-formed.
    pub fn validate<P: AsRef<Path>>(apk_path: P) -> NeutronResult<bool> {
        let file = std::fs::File::open(apk_path.as_ref())?;
        let archive = ZipArchive::new(file)
            .map_err(|e| NeutronError::ApkParse(format!("Invalid zip: {}", e)))?;

        // Must contain AndroidManifest.xml and classes.dex
        let has_manifest = (0..archive.len()).any(|i| {
            archive.name_for_index(i)
                .map(|n| n == "AndroidManifest.xml")
                .unwrap_or(false)
        });

        let has_dex = (0..archive.len()).any(|i| {
            archive.name_for_index(i)
                .map(|n| n == "classes.dex" || n.starts_with("classes") && n.ends_with(".dex"))
                .unwrap_or(false)
        });

        Ok(has_manifest && has_dex)
    }

    // --- Private ---

    fn find_native_libs(archive: &mut ZipArchive<std::fs::File>) -> NeutronResult<Vec<NativeLibInfo>> {
        let mut libs = Vec::new();

        for i in 0..archive.len() {
            let entry = archive.by_index(i)
                .map_err(|e| NeutronError::ApkParse(e.to_string()))?;
            
            let name = entry.name().to_string();
            if name.starts_with("lib/") && name.ends_with(".so") {
                let parts: Vec<&str> = name.split('/').collect();
                if parts.len() >= 3 {
                    let abi = match parts[1] {
                        "arm64-v8a" => NativeAbi::Arm64V8a,
                        "armeabi-v7a" => NativeAbi::ArmeabiV7a,
                        _ => continue,
                    };

                    libs.push(NativeLibInfo {
                        name: parts[2].to_string(),
                        abi,
                        path_in_apk: name,
                        size: entry.size(),
                    });
                }
            }
        }

        Ok(libs)
    }

    fn detect_abi(libs: &[NativeLibInfo]) -> NativeAbi {
        let has_64 = libs.iter().any(|l| l.abi == NativeAbi::Arm64V8a);
        let has_32 = libs.iter().any(|l| l.abi == NativeAbi::ArmeabiV7a);

        match (has_64, has_32) {
            (true, true) => NativeAbi::Universal,
            (true, false) => NativeAbi::Arm64V8a,
            (false, true) => NativeAbi::ArmeabiV7a,
            (false, false) => NativeAbi::Arm64V8a, // Default
        }
    }

    fn parse_manifest(archive: &mut ZipArchive<std::fs::File>) -> NeutronResult<ManifestInfo> {
        // Android's binary XML format — we do a simplified parse
        // to extract package name, version, and SDK requirements.
        
        let manifest_entry = archive.by_name("AndroidManifest.xml")
            .map_err(|_| NeutronError::ApkParse("No AndroidManifest.xml found".into()))?;

        // Binary XML parsing (simplified — looks for string pool entries)
        let mut data = Vec::new();
        let mut reader = std::io::BufReader::new(manifest_entry);
        reader.read_to_end(&mut data)?;

        // Extract strings from the binary XML string pool
        let manifest = ManifestInfo::from_binary_xml(&data)?;
        Ok(manifest)
    }

    fn compute_sha256<P: AsRef<Path>>(path: P) -> NeutronResult<String> {
        let data = std::fs::read(path.as_ref())?;
        let hash = Sha256::digest(&data);
        Ok(format!("{:x}", hash))
    }
}
