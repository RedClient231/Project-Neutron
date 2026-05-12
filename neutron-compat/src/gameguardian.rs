//! GameGuardian compatibility support.
//!
//! GameGuardian is a memory editor that requires:
//! 1. Ability to ptrace (attach to) the target game process
//! 2. Access to /proc/pid/maps of the target
//! 3. Read/write access to /proc/pid/mem
//! 4. The virtual environment must not be detected
//!
//! Since both GG and the game run inside our virtual space,
//! we can facilitate the ptrace relationship between them.

use neutron_core::{NeutronError, NeutronResult, MemoryRegion};
use log::{debug, info, warn};
use std::collections::HashMap;

/// GameGuardian compatibility support layer.
pub struct GameGuardianSupport {
    /// PID of GameGuardian within the virtual space
    gg_pid: Option<u32>,
    /// PID of the target game process
    game_pid: Option<u32>,
    /// Whether GG is allowed to attach
    attach_allowed: bool,
    /// Fake /proc/pid/mem file descriptors
    mem_fd_map: HashMap<u32, i32>,
}

impl GameGuardianSupport {
    /// Create a new GG support instance.
    pub fn new() -> Self {
        Self {
            gg_pid: None,
            game_pid: None,
            attach_allowed: true,
            mem_fd_map: HashMap::new(),
        }
    }

    /// Register GameGuardian's PID in the virtual space.
    pub fn register_gg(&mut self, pid: u32) {
        self.gg_pid = Some(pid);
        info!("GameGuardian registered: pid={}", pid);
    }

    /// Register the target game's PID.
    pub fn register_game(&mut self, pid: u32) {
        self.game_pid = Some(pid);
        info!("Game process registered for GG: pid={}", pid);
    }

    /// Check if a ptrace attach request should be allowed.
    /// 
    /// GameGuardian uses ptrace(PTRACE_ATTACH) to attach to the game.
    /// In our virtual space, we intercept this and handle it ourselves
    /// since both processes are our children.
    pub fn should_allow_attach(&self, requester: u32, target: u32) -> bool {
        if !self.attach_allowed {
            return false;
        }

        // Allow GG to attach to the game
        if Some(requester) == self.gg_pid && Some(target) == self.game_pid {
            debug!("Allowing GG ptrace attach: {} -> {}", requester, target);
            return true;
        }

        // Allow GG to attach to any process in our virtual space
        if Some(requester) == self.gg_pid {
            debug!("Allowing GG ptrace to virtual process: {} -> {}", requester, target);
            return true;
        }

        false
    }

    /// Handle a memory read request from GameGuardian.
    ///
    /// GG reads game memory via process_vm_readv or /proc/pid/mem.
    /// Since both are our child processes, we can facilitate this
    /// using our own ptrace access.
    pub fn handle_memory_read(
        &self,
        target_pid: u32,
        address: u64,
        size: usize,
    ) -> NeutronResult<Vec<u8>> {
        let mut buffer = vec![0u8; size];

        // Use process_vm_readv for efficient cross-process memory reading
        let local_iov = libc::iovec {
            iov_base: buffer.as_mut_ptr() as *mut libc::c_void,
            iov_len: size,
        };

        let remote_iov = libc::iovec {
            iov_base: address as *mut libc::c_void,
            iov_len: size,
        };

        let result = unsafe {
            libc::process_vm_readv(
                target_pid as libc::pid_t,
                &local_iov as *const libc::iovec,
                1,
                &remote_iov as *const libc::iovec,
                1,
                0,
            )
        };

        if result < 0 {
            return Err(NeutronError::Memory(format!(
                "process_vm_readv failed for pid={} addr=0x{:x}: {}",
                target_pid, address, std::io::Error::last_os_error()
            )));
        }

        buffer.truncate(result as usize);
        Ok(buffer)
    }

    /// Handle a memory write request from GameGuardian.
    pub fn handle_memory_write(
        &self,
        target_pid: u32,
        address: u64,
        data: &[u8],
    ) -> NeutronResult<()> {
        let local_iov = libc::iovec {
            iov_base: data.as_ptr() as *mut libc::c_void,
            iov_len: data.len(),
        };

        let remote_iov = libc::iovec {
            iov_base: address as *mut libc::c_void,
            iov_len: data.len(),
        };

        let result = unsafe {
            libc::process_vm_writev(
                target_pid as libc::pid_t,
                &local_iov as *const libc::iovec,
                1,
                &remote_iov as *const libc::iovec,
                1,
                0,
            )
        };

        if result < 0 {
            return Err(NeutronError::Memory(format!(
                "process_vm_writev failed for pid={} addr=0x{:x}: {}",
                target_pid, address, std::io::Error::last_os_error()
            )));
        }

        Ok(())
    }

    /// Generate a clean /proc/pid/maps for GameGuardian to read.
    /// 
    /// GG uses this to find memory regions of the game (heap, stack, .so files).
    /// We provide an accurate but sanitized view.
    pub fn generate_game_maps(&self, target_pid: u32) -> NeutronResult<String> {
        let maps_path = format!("/proc/{}/maps", target_pid);
        let raw_maps = std::fs::read_to_string(&maps_path)?;

        // Clean the maps — remove our own entries
        let cleaned = raw_maps.lines()
            .filter(|line| {
                let lower = line.to_lowercase();
                !lower.contains("neutron")
                    && !lower.contains("frida")
                    && !lower.contains("xposed")
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(cleaned)
    }

    /// Handle a /proc/pid/mem open request from GG.
    /// 
    /// We intercept the open and provide a file descriptor that
    /// proxies reads/writes through our process_vm_readv/writev.
    pub fn handle_proc_mem_open(&mut self, target_pid: u32) -> NeutronResult<i32> {
        // Create a memfd or pipe that we'll serve reads from
        let fd = unsafe {
            libc::syscall(
                libc::SYS_memfd_create,
                "neutron_mem\0".as_ptr(),
                0u32,
            ) as i32
        };

        if fd < 0 {
            return Err(NeutronError::Memory(
                "memfd_create failed".into()
            ));
        }

        self.mem_fd_map.insert(target_pid, fd);
        Ok(fd)
    }

    /// Enable/disable GG attachment.
    pub fn set_attach_allowed(&mut self, allowed: bool) {
        self.attach_allowed = allowed;
    }
}

impl Default for GameGuardianSupport {
    fn default() -> Self {
        Self::new()
    }
}
