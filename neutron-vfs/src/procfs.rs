//! /proc filesystem spoofing for virtual processes.
//!
//! Critical for:
//! - GameGuardian compatibility (needs to read /proc/pid/maps)
//! - Anti-detection (hide our process from the guest)
//! - Memory layout spoofing

use neutron_core::{MemoryRegion, NeutronResult, NeutronError};
use log::{debug, trace};

/// Handles spoofing of /proc entries for virtualized processes.
#[derive(Debug)]
pub struct ProcfsSpoofing {
    /// PID of the virtualized process
    target_pid: u32,
    /// Our host PID (to hide from guest)
    host_pid: u32,
    /// Custom memory regions to report in /proc/pid/maps
    spoofed_maps: Vec<MemoryRegion>,
    /// Whether to allow ptrace attach from guest tools (GameGuardian)
    allow_ptrace: bool,
}

impl ProcfsSpoofing {
    /// Create a new proc spoofing handler.
    pub fn new(target_pid: u32, host_pid: u32, allow_ptrace: bool) -> Self {
        Self {
            target_pid,
            host_pid,
            spoofed_maps: Vec::new(),
            allow_ptrace,
        }
    }

    /// Generate spoofed /proc/self/maps content.
    /// 
    /// This removes traces of Neutron from the memory map and presents
    /// a clean map that looks like a normal app process. Essential for
    /// GameGuardian which reads maps to find memory regions.
    pub fn generate_maps(&self, real_maps: &str) -> String {
        let mut output = String::with_capacity(real_maps.len());

        for line in real_maps.lines() {
            // Filter out our own libraries
            if self.should_hide_map_entry(line) {
                continue;
            }

            // Rewrite paths if needed
            let processed = self.rewrite_map_entry(line);
            output.push_str(&processed);
            output.push('\n');
        }

        // Append any spoofed regions
        for region in &self.spoofed_maps {
            output.push_str(&Self::format_map_entry(region));
            output.push('\n');
        }

        output
    }

    /// Generate spoofed /proc/self/status content.
    pub fn generate_status(&self, package_name: &str) -> String {
        format!(
            "Name:\t{name}\n\
             Umask:\t0077\n\
             State:\tS (sleeping)\n\
             Tgid:\t{pid}\n\
             Ngid:\t0\n\
             Pid:\t{pid}\n\
             PPid:\t1\n\
             TracerPid:\t0\n\
             Uid:\t10001\t10001\t10001\t10001\n\
             Gid:\t10001\t10001\t10001\t10001\n\
             FDSize:\t512\n\
             VmPeak:\t  2048000 kB\n\
             VmSize:\t  1920000 kB\n\
             VmRSS:\t   384000 kB\n\
             Threads:\t32\n",
            name = package_name.split('.').last().unwrap_or("app"),
            pid = self.target_pid,
        )
    }

    /// Generate spoofed /proc/self/cmdline.
    pub fn generate_cmdline(&self, package_name: &str) -> Vec<u8> {
        let mut cmdline = package_name.as_bytes().to_vec();
        cmdline.push(0);
        cmdline
    }

    /// Add a spoofed memory region to the map.
    pub fn add_spoofed_region(&mut self, region: MemoryRegion) {
        self.spoofed_maps.push(region);
    }

    /// Check if ptrace should be allowed from within the virtual space.
    /// GameGuardian needs this to attach to game processes.
    pub fn should_allow_ptrace(&self, requester_pid: u32, target_pid: u32) -> bool {
        if !self.allow_ptrace {
            return false;
        }
        // Allow ptrace within the virtual space only
        // (GameGuardian attaching to the game process)
        debug!(
            "Ptrace request: {} -> {} (allowed: {})",
            requester_pid, target_pid, true
        );
        true
    }

    // --- Private ---

    fn should_hide_map_entry(&self, line: &str) -> bool {
        let hidden_indicators = [
            "neutron",
            "frida",
            "xposed",
            "magisk",
            "substrate",
            "cydia",
            "com.neutron.virtualspace",
        ];

        let lower = line.to_lowercase();
        hidden_indicators.iter().any(|h| lower.contains(h))
    }

    fn rewrite_map_entry(&self, line: &str) -> String {
        // No rewriting needed for most entries
        line.to_string()
    }

    fn format_map_entry(region: &MemoryRegion) -> String {
        format!(
            "{:012x}-{:012x} {} {:08x} {} {} {}",
            region.start,
            region.end,
            region.permissions,
            region.offset,
            region.device,
            region.inode,
            region.pathname,
        )
    }
}
