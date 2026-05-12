//! Function hooking via inline assembly trampolines.
//!
//! Provides the ability to intercept function calls within the virtualized
//! process by overwriting the function prologue with a branch to our handler.
//! Critical for:
//! - Intercepting dlopen/dlsym for library loading
//! - Hooking Android framework functions
//! - Supporting GameGuardian's memory scanning

use neutron_core::{NeutronError, NeutronResult};
use log::{debug, trace, warn};

/// Size of the hook trampoline in bytes (ARM64).
const TRAMPOLINE_SIZE_ARM64: usize = 16;

/// Size of the hook trampoline in bytes (ARM32).
const TRAMPOLINE_SIZE_ARM32: usize = 12;

/// Represents an installed inline hook.
#[derive(Debug, Clone)]
pub struct InlineHook {
    /// Address of the hooked function
    pub target_addr: u64,
    /// Original bytes that were overwritten
    pub original_bytes: Vec<u8>,
    /// Address of the hook handler
    pub handler_addr: u64,
    /// Whether the hook is currently active
    pub active: bool,
}

/// Hook manager for installing/removing inline hooks via ptrace.
pub struct HookManager {
    /// PID of the traced process
    pid: u32,
    /// Installed hooks
    hooks: Vec<InlineHook>,
}

impl HookManager {
    /// Create a new hook manager for the given process.
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            hooks: Vec::new(),
        }
    }

    /// Install an inline hook at the given address.
    /// 
    /// ARM64 trampoline (16 bytes):
    /// ```asm
    ///   ldr x16, [pc, #8]   // Load handler address from next 8 bytes
    ///   br x16              // Branch to handler
    ///   .quad handler_addr  // 8-byte handler address
    /// ```
    pub fn install_hook_arm64(&mut self, target: u64, handler: u64) -> NeutronResult<()> {
        // Read original bytes
        let original = self.read_memory(target, TRAMPOLINE_SIZE_ARM64)?;

        // Build trampoline
        let trampoline = Self::build_arm64_trampoline(handler);

        // Write trampoline to target
        self.write_memory(target, &trampoline)?;

        self.hooks.push(InlineHook {
            target_addr: target,
            original_bytes: original,
            handler_addr: handler,
            active: true,
        });

        debug!("ARM64 hook installed: 0x{:x} -> 0x{:x}", target, handler);
        Ok(())
    }

    /// Install an inline hook for ARM32.
    /// 
    /// ARM32 trampoline (12 bytes):
    /// ```asm
    ///   ldr pc, [pc, #0]    // Load handler address
    ///   .word handler_addr  // 4-byte handler address
    /// ```
    pub fn install_hook_arm32(&mut self, target: u64, handler: u64) -> NeutronResult<()> {
        let original = self.read_memory(target, TRAMPOLINE_SIZE_ARM32)?;
        let trampoline = Self::build_arm32_trampoline(handler as u32);
        self.write_memory(target, &trampoline)?;

        self.hooks.push(InlineHook {
            target_addr: target,
            original_bytes: original,
            handler_addr: handler,
            active: true,
        });

        debug!("ARM32 hook installed: 0x{:x} -> 0x{:x}", target, handler);
        Ok(())
    }

    /// Remove a hook and restore original bytes.
    pub fn remove_hook(&mut self, target: u64) -> NeutronResult<()> {
        if let Some(pos) = self.hooks.iter().position(|h| h.target_addr == target) {
            let hook = &self.hooks[pos];
            self.write_memory(target, &hook.original_bytes)?;
            self.hooks.remove(pos);
            debug!("Hook removed at 0x{:x}", target);
        }
        Ok(())
    }

    /// Remove all hooks.
    pub fn remove_all_hooks(&mut self) -> NeutronResult<()> {
        let hooks: Vec<_> = self.hooks.iter().map(|h| (h.target_addr, h.original_bytes.clone())).collect();
        for (addr, bytes) in hooks {
            self.write_memory(addr, &bytes)?;
        }
        self.hooks.clear();
        Ok(())
    }

    // --- Private ---

    /// Build ARM64 trampoline bytes.
    fn build_arm64_trampoline(handler: u64) -> Vec<u8> {
        let mut trampoline = Vec::with_capacity(TRAMPOLINE_SIZE_ARM64);
        
        // LDR X16, [PC, #8] — encoded as: 0x58000050
        trampoline.extend_from_slice(&0x58000050u32.to_le_bytes());
        // BR X16 — encoded as: 0xD61F0200
        trampoline.extend_from_slice(&0xD61F0200u32.to_le_bytes());
        // 8-byte handler address
        trampoline.extend_from_slice(&handler.to_le_bytes());
        
        trampoline
    }

    /// Build ARM32 trampoline bytes.
    fn build_arm32_trampoline(handler: u32) -> Vec<u8> {
        let mut trampoline = Vec::with_capacity(TRAMPOLINE_SIZE_ARM32);
        
        // LDR PC, [PC, #0] — encoded as: 0xE51FF004 
        // (actually LDR PC, [PC, #-4] accounting for ARM pipeline)
        trampoline.extend_from_slice(&0xE51FF004u32.to_le_bytes());
        // 4-byte handler address
        trampoline.extend_from_slice(&handler.to_le_bytes());
        // NOP padding
        trampoline.extend_from_slice(&0xE1A00000u32.to_le_bytes());
        
        trampoline
    }

    /// Read memory from the traced process.
    fn read_memory(&self, addr: u64, len: usize) -> NeutronResult<Vec<u8>> {
        let mut result = Vec::with_capacity(len);
        let word_size = core::mem::size_of::<libc::c_long>();
        let mut offset = 0;

        while offset < len {
            let word = unsafe {
                libc::ptrace(
                    libc::PTRACE_PEEKDATA,
                    self.pid as libc::pid_t,
                    (addr + offset as u64) as *mut libc::c_void,
                    core::ptr::null_mut::<libc::c_void>(),
                )
            };

            let bytes = word.to_ne_bytes();
            let remaining = len - offset;
            let to_copy = remaining.min(word_size);
            result.extend_from_slice(&bytes[..to_copy]);
            offset += word_size;
        }

        Ok(result)
    }

    /// Write memory to the traced process.
    fn write_memory(&self, addr: u64, data: &[u8]) -> NeutronResult<()> {
        let word_size = core::mem::size_of::<libc::c_long>();
        let mut offset = 0;

        while offset < data.len() {
            let mut word: libc::c_long = 0;
            
            // If partial word, read existing data first
            if data.len() - offset < word_size {
                word = unsafe {
                    libc::ptrace(
                        libc::PTRACE_PEEKDATA,
                        self.pid as libc::pid_t,
                        (addr + offset as u64) as *mut libc::c_void,
                        core::ptr::null_mut::<libc::c_void>(),
                    )
                };
            }

            let bytes = unsafe {
                core::slice::from_raw_parts_mut(
                    &mut word as *mut _ as *mut u8,
                    word_size,
                )
            };

            let remaining = data.len() - offset;
            let to_copy = remaining.min(word_size);
            bytes[..to_copy].copy_from_slice(&data[offset..offset + to_copy]);

            let result = unsafe {
                libc::ptrace(
                    libc::PTRACE_POKEDATA,
                    self.pid as libc::pid_t,
                    (addr + offset as u64) as *mut libc::c_void,
                    word as *mut libc::c_void,
                )
            };

            if result < 0 {
                return Err(NeutronError::Hook {
                    addr: addr + offset as u64,
                    reason: format!("POKEDATA failed: {}", std::io::Error::last_os_error()),
                });
            }

            offset += word_size;
        }

        Ok(())
    }
}
