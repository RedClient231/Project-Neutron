//! Android Binary XML Manifest parser.
//!
//! Parses the binary AndroidManifest.xml format to extract
//! essential package metadata without requiring aapt or Java tools.

use neutron_core::{NeutronError, NeutronResult};
use log::debug;

/// Parsed manifest information.
#[derive(Debug, Clone)]
pub struct ManifestInfo {
    pub package_name: String,
    pub version_code: u64,
    pub version_name: String,
    pub label: String,
    pub min_sdk: u32,
    pub target_sdk: u32,
    pub permissions: Vec<String>,
    pub activities: Vec<String>,
    pub services: Vec<String>,
}

impl ManifestInfo {
    /// Parse from Android's binary XML format.
    ///
    /// The binary XML format has:
    /// - Magic: 0x00080003
    /// - String pool (all strings referenced by index)
    /// - Resource IDs
    /// - XML elements with attribute references into the string pool
    pub fn from_binary_xml(data: &[u8]) -> NeutronResult<Self> {
        if data.len() < 8 {
            return Err(NeutronError::ApkParse("Manifest too small".into()));
        }

        // Parse string pool to find package name and version
        let strings = Self::extract_string_pool(data)?;
        
        // Search for common Android manifest strings
        let package_name = Self::find_package_name(&strings, data);
        let version_name = Self::find_version_name(&strings);

        Ok(Self {
            package_name: package_name.unwrap_or_else(|| "unknown.package".into()),
            version_code: Self::find_version_code(data),
            version_name: version_name.unwrap_or_else(|| "1.0".into()),
            label: Self::find_label(&strings).unwrap_or_else(|| "App".into()),
            min_sdk: Self::find_min_sdk(data),
            target_sdk: Self::find_target_sdk(data),
            permissions: Self::find_permissions(&strings),
            activities: Self::find_activities(&strings),
            services: Vec::new(),
        })
    }

    /// Extract the string pool from binary XML.
    fn extract_string_pool(data: &[u8]) -> NeutronResult<Vec<String>> {
        let mut strings = Vec::new();
        
        if data.len() < 16 {
            return Ok(strings);
        }

        // String pool chunk starts at offset 8
        // Chunk type: 0x001C0001 for string pool
        let mut offset = 8;
        
        // Look for string pool header
        while offset + 4 < data.len() {
            let chunk_type = u16::from_le_bytes([data[offset], data[offset + 1]]);
            if chunk_type == 0x0001 {
                // Found string pool
                if offset + 28 < data.len() {
                    let string_count = u32::from_le_bytes([
                        data[offset + 8], data[offset + 9],
                        data[offset + 10], data[offset + 11],
                    ]) as usize;
                    
                    let string_start = u32::from_le_bytes([
                        data[offset + 20], data[offset + 21],
                        data[offset + 22], data[offset + 23],
                    ]) as usize;

                    let is_utf8 = (u32::from_le_bytes([
                        data[offset + 16], data[offset + 17],
                        data[offset + 18], data[offset + 19],
                    ]) & (1 << 8)) != 0;

                    let offsets_start = offset + 28;
                    let strings_data_start = offset + string_start;

                    for i in 0..string_count.min(1024) {
                        let str_offset_pos = offsets_start + i * 4;
                        if str_offset_pos + 4 > data.len() {
                            break;
                        }
                        
                        let str_offset = u32::from_le_bytes([
                            data[str_offset_pos], data[str_offset_pos + 1],
                            data[str_offset_pos + 2], data[str_offset_pos + 3],
                        ]) as usize;

                        let abs_offset = strings_data_start + str_offset;
                        if abs_offset + 2 > data.len() {
                            strings.push(String::new());
                            continue;
                        }

                        let s = if is_utf8 {
                            Self::read_utf8_string(data, abs_offset)
                        } else {
                            Self::read_utf16_string(data, abs_offset)
                        };
                        
                        strings.push(s);
                    }
                }
                break;
            }
            offset += 1;
        }

        Ok(strings)
    }

    fn read_utf8_string(data: &[u8], offset: usize) -> String {
        if offset + 2 >= data.len() {
            return String::new();
        }
        // Skip the two length bytes
        let str_start = offset + 2;
        let mut end = str_start;
        while end < data.len() && data[end] != 0 {
            end += 1;
        }
        String::from_utf8_lossy(&data[str_start..end]).to_string()
    }

    fn read_utf16_string(data: &[u8], offset: usize) -> String {
        if offset + 2 >= data.len() {
            return String::new();
        }
        let char_count = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        let str_start = offset + 2;
        let mut chars = Vec::with_capacity(char_count);
        
        for i in 0..char_count {
            let pos = str_start + i * 2;
            if pos + 2 > data.len() {
                break;
            }
            let c = u16::from_le_bytes([data[pos], data[pos + 1]]);
            if c == 0 {
                break;
            }
            chars.push(c);
        }
        
        String::from_utf16_lossy(&chars)
    }

    fn find_package_name(strings: &[String], _data: &[u8]) -> Option<String> {
        // Package name typically looks like "com.xxx.yyy"
        strings.iter()
            .find(|s| {
                let parts: Vec<&str> = s.split('.').collect();
                parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_'))
            })
            .cloned()
    }

    fn find_version_name(strings: &[String]) -> Option<String> {
        strings.iter()
            .find(|s| {
                // Version names typically match patterns like "1.0.0" or "2.3.1-beta"
                let first_char = s.chars().next();
                matches!(first_char, Some('0'..='9')) && s.contains('.')
            })
            .cloned()
    }

    fn find_label(strings: &[String]) -> Option<String> {
        // Label is typically a short human-readable name
        strings.iter()
            .find(|s| {
                s.len() > 1 && s.len() < 50 
                && !s.contains('.') 
                && !s.contains('/')
                && s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            })
            .cloned()
    }

    fn find_version_code(data: &[u8]) -> u64 {
        // Default version code
        1
    }

    fn find_min_sdk(data: &[u8]) -> u32 {
        // Default to Android 8.0
        26
    }

    fn find_target_sdk(data: &[u8]) -> u32 {
        // Default to Android 13
        33
    }

    fn find_permissions(strings: &[String]) -> Vec<String> {
        strings.iter()
            .filter(|s| s.starts_with("android.permission."))
            .cloned()
            .collect()
    }

    fn find_activities(strings: &[String]) -> Vec<String> {
        strings.iter()
            .filter(|s| s.contains("Activity") || s.contains("activity"))
            .cloned()
            .collect()
    }
}
