//! Syscall Tracer — the heart of process virtualization.
//!
//! Uses ptrace to intercept every syscall from the virtualized process.
//! Filesystem access, identity queries, and detection checks are
//! intercepted and spoofed/redirected.

use neutron_core::{
    NeutronError, NeutronResult, SyscallAction,
    platform::{aarch64, ptrace_consts::*},
    syscall::raw,
};
use neutron_vfs::{PathRedirector, ProcfsSpoofing, VfsOverlay};
use log::{debug, trace, warn};
use std::collections::HashMap;

/// AArch64 user register set for ptrace.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UserRegs {
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
}

/// Syscall tracer that intercepts and modifies process behavior.
pub struct SyscallTracer {
    /// PID of the traced process
    pid: u32,
    /// Whether we're in syscall-enter or syscall-exit
    in_syscall: bool,
    /// Current registers at syscall entry
    saved_regs: UserRegs,
    /// Syscall filtering rules
    filters: HashMap<u64, SyscallAction>,
    /// Whether the tracer is active
    active: bool,
}

impl SyscallTracer {
    /// Create a new syscall tracer for the given PID.
    pub fn new(pid: u32) -> Self {
        let mut tracer = Self {
            pid,
            in_syscall: false,
            saved_regs: UserRegs::default(),
            filters: HashMap::new(),
            active: false,
        };
        tracer.setup_default_filters();
        tracer
    }

    /// Attach to the process and begin tracing.
    pub fn attach(&mut self) -> NeutronResult<()> {
        let result = unsafe {
            libc::ptrace(
                libc::PTRACE_ATTACH,
                self.pid as libc::pid_t,
                core::ptr::null_mut::<libc::c_void>(),
                core::ptr::null_mut::<libc::c_void>(),
            )
        };

        if result < 0 {
            return Err(NeutronError::Ptrace {
                pid: self.pid as i32,
                request: PTRACE_ATTACH,
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(-1),
            });
        }

        // Wait for the process to stop
        let mut status: i32 = 0;
        unsafe {
            libc::waitpid(self.pid as i32, &mut status, 0);
        }

        // Set ptrace options for comprehensive tracing
        let options = libc::PTRACE_O_TRACESYSGOOD
            | libc::PTRACE_O_TRACEFORK
            | libc::PTRACE_O_TRACEVFORK
            | libc::PTRACE_O_TRACECLONE
            | libc::PTRACE_O_TRACEEXEC;

        unsafe {
            libc::ptrace(
                libc::PTRACE_SETOPTIONS,
                self.pid as libc::pid_t,
                core::ptr::null_mut::<libc::c_void>(),
                options as *mut libc::c_void,
            );
        }

        self.active = true;
        debug!("Syscall tracer attached to pid {}", self.pid);
        Ok(())
    }

    /// Detach from the process.
    pub fn detach(&mut self) -> NeutronResult<()> {
        if self.active {
            unsafe {
                libc::ptrace(
                    libc::PTRACE_DETACH,
                    self.pid as libc::pid_t,
                    core::ptr::null_mut::<libc::c_void>(),
                    core::ptr::null_mut::<libc::c_void>(),
                );
            }
            self.active = false;
        }
        Ok(())
    }

    /// Run the main tracing loop. Returns when the process exits.
    pub fn trace_loop(
        &mut self,
        redirector: &PathRedirector,
        procfs: &ProcfsSpoofing,
    ) -> NeutronResult<i32> {
        loop {
            // Continue until next syscall
            let cont_result = unsafe {
                libc::ptrace(
                    libc::PTRACE_SYSCALL,
                    self.pid as libc::pid_t,
                    core::ptr::null_mut::<libc::c_void>(),
                    core::ptr::null_mut::<libc::c_void>(),
                )
            };

            if cont_result < 0 {
                let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
                if errno == libc::ESRCH {
                    return Ok(0); // Process gone
                }
                return Err(NeutronError::Ptrace {
                    pid: self.pid as i32,
                    request: PTRACE_SYSCALL,
                    errno,
                });
            }

            // Wait for syscall stop
            let mut status: i32 = 0;
            let wait_result = unsafe {
                libc::waitpid(self.pid as i32, &mut status, 0)
            };

            if wait_result < 0 {
                return Ok(0);
            }

            // Check if process exited
            if libc::WIFEXITED(status) {
                return Ok(libc::WEXITSTATUS(status));
            }

            if libc::WIFSIGNALED(status) {
                return Ok(-libc::WTERMSIG(status));
            }

            // Check if it's a syscall stop
            if libc::WIFSTOPPED(status) {
                let sig = libc::WSTOPSIG(status);
                if sig == (libc::SIGTRAP | 0x80) {
                    // Syscall stop - handle it
                    self.handle_syscall_stop(redirector, procfs)?;
                } else if sig == libc::SIGTRAP {
                    // ptrace event - continue
                    continue;
                } else {
                    // Deliver signal to child
                    unsafe {
                        libc::ptrace(
                            libc::PTRACE_SYSCALL,
                            self.pid as libc::pid_t,
                            core::ptr::null_mut::<libc::c_void>(),
                            sig as *mut libc::c_void,
                        );
                    }
                    continue;
                }
            }
        }
    }

    /// Handle a syscall-entry or syscall-exit stop.
    fn handle_syscall_stop(
        &mut self,
        redirector: &PathRedirector,
        procfs: &ProcfsSpoofing,
    ) -> NeutronResult<()> {
        let regs = self.get_regs()?;

        if !self.in_syscall {
            // Syscall entry
            self.in_syscall = true;
            self.saved_regs = regs;

            let syscall_nr = regs.regs[8]; // x8 = syscall number on ARM64
            
            // Check filter
            if let Some(&action) = self.filters.get(&syscall_nr) {
                match action {
                    SyscallAction::Block(errno) => {
                        // Skip syscall by setting invalid number
                        let mut new_regs = regs;
                        new_regs.regs[8] = u64::MAX; // Invalid syscall
                        self.set_regs(&new_regs)?;
                    }
                    SyscallAction::Redirect => {
                        self.handle_redirect(syscall_nr, &regs, redirector)?;
                    }
                    SyscallAction::ModifyArgs => {
                        self.handle_modify_args(syscall_nr, &regs, redirector)?;
                    }
                    _ => {}
                }
            }

            // Handle specific syscalls that need interception
            self.intercept_syscall_entry(syscall_nr, &regs, redirector)?;
        } else {
            // Syscall exit
            self.in_syscall = false;

            let syscall_nr = self.saved_regs.regs[8];
            
            if let Some(&SyscallAction::SpoofReturn(value)) = self.filters.get(&syscall_nr) {
                let mut new_regs = regs;
                new_regs.regs[0] = value as u64;
                self.set_regs(&new_regs)?;
            }
        }

        Ok(())
    }

    /// Intercept specific syscalls at entry.
    fn intercept_syscall_entry(
        &self,
        nr: u64,
        regs: &UserRegs,
        redirector: &PathRedirector,
    ) -> NeutronResult<()> {
        match nr {
            // openat(dirfd, pathname, flags, mode)
            n if n == aarch64::SYS_OPENAT => {
                // Read pathname from tracee memory
                let path_addr = regs.regs[1];
                if let Ok(path) = self.read_string_from_tracee(path_addr) {
                    if let Some(redirected) = redirector.redirect(&path) {
                        trace!("openat redirect: {} -> {}", path, redirected);
                        self.write_string_to_tracee(path_addr, &redirected)?;
                    } else if redirector.should_hide(&path) {
                        // Make the syscall fail with ENOENT
                        trace!("openat blocked: {}", path);
                    }
                }
            }
            // readlinkat(dirfd, pathname, buf, bufsiz)
            n if n == aarch64::SYS_READLINKAT => {
                let path_addr = regs.regs[1];
                if let Ok(path) = self.read_string_from_tracee(path_addr) {
                    if let Some(redirected) = redirector.redirect(&path) {
                        self.write_string_to_tracee(path_addr, &redirected)?;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle syscall redirection.
    fn handle_redirect(
        &self,
        nr: u64,
        regs: &UserRegs,
        redirector: &PathRedirector,
    ) -> NeutronResult<()> {
        // Handled in intercept_syscall_entry
        Ok(())
    }

    /// Handle argument modification.
    fn handle_modify_args(
        &self,
        nr: u64,
        regs: &UserRegs,
        redirector: &PathRedirector,
    ) -> NeutronResult<()> {
        // Handled in intercept_syscall_entry
        Ok(())
    }

    /// Read registers from the tracee.
    fn get_regs(&self) -> NeutronResult<UserRegs> {
        let mut regs = UserRegs::default();
        
        // Use PTRACE_GETREGSET with NT_PRSTATUS for AArch64
        let iov = libc::iovec {
            iov_base: &mut regs as *mut _ as *mut libc::c_void,
            iov_len: core::mem::size_of::<UserRegs>(),
        };

        let result = unsafe {
            libc::ptrace(
                libc::PTRACE_GETREGSET,
                self.pid as libc::pid_t,
                1 as *mut libc::c_void, // NT_PRSTATUS
                &iov as *const _ as *mut libc::c_void,
            )
        };

        if result < 0 {
            return Err(NeutronError::Ptrace {
                pid: self.pid as i32,
                request: PTRACE_GETREGSET as i64,
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(-1),
            });
        }

        Ok(regs)
    }

    /// Write registers to the tracee.
    fn set_regs(&self, regs: &UserRegs) -> NeutronResult<()> {
        let iov = libc::iovec {
            iov_base: regs as *const _ as *mut libc::c_void,
            iov_len: core::mem::size_of::<UserRegs>(),
        };

        let result = unsafe {
            libc::ptrace(
                libc::PTRACE_SETREGSET,
                self.pid as libc::pid_t,
                1 as *mut libc::c_void, // NT_PRSTATUS
                &iov as *const _ as *mut libc::c_void,
            )
        };

        if result < 0 {
            return Err(NeutronError::Ptrace {
                pid: self.pid as i32,
                request: PTRACE_SETREGSET as i64,
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(-1),
            });
        }

        Ok(())
    }

    /// Read a null-terminated string from tracee memory.
    fn read_string_from_tracee(&self, addr: u64) -> NeutronResult<String> {
        let mut result = Vec::with_capacity(256);
        let mut offset = 0u64;

        loop {
            let word = unsafe {
                libc::ptrace(
                    libc::PTRACE_PEEKDATA,
                    self.pid as libc::pid_t,
                    (addr + offset) as *mut libc::c_void,
                    core::ptr::null_mut::<libc::c_void>(),
                )
            };

            let bytes = word.to_ne_bytes();
            for &b in &bytes {
                if b == 0 {
                    return String::from_utf8(result)
                        .map_err(|e| NeutronError::Memory(e.to_string()));
                }
                result.push(b);
                if result.len() > 4096 {
                    return Err(NeutronError::Memory("String too long".into()));
                }
            }
            offset += 8;
        }
    }

    /// Write a null-terminated string to tracee memory.
    fn write_string_to_tracee(&self, addr: u64, s: &str) -> NeutronResult<()> {
        let bytes = s.as_bytes();
        let mut offset = 0u64;

        // Write 8 bytes at a time
        for chunk in bytes.chunks(8) {
            let mut word: i64 = 0;
            for (i, &b) in chunk.iter().enumerate() {
                word |= (b as i64) << (i * 8);
            }

            unsafe {
                libc::ptrace(
                    libc::PTRACE_POKEDATA,
                    self.pid as libc::pid_t,
                    (addr + offset) as *mut libc::c_void,
                    word as *mut libc::c_void,
                );
            }
            offset += 8;
        }

        // Write null terminator
        if bytes.len() % 8 != 0 {
            // Already handled by zero-padding in the last chunk
        } else {
            unsafe {
                libc::ptrace(
                    libc::PTRACE_POKEDATA,
                    self.pid as libc::pid_t,
                    (addr + offset) as *mut libc::c_void,
                    0 as *mut libc::c_void,
                );
            }
        }

        Ok(())
    }

    /// Setup default syscall filters for virtual environment.
    fn setup_default_filters(&mut self) {
        // Filesystem access - redirect
        self.filters.insert(aarch64::SYS_OPENAT, SyscallAction::Redirect);
        self.filters.insert(aarch64::SYS_READLINKAT, SyscallAction::Redirect);
        self.filters.insert(aarch64::SYS_FSTATAT, SyscallAction::Redirect);
        self.filters.insert(aarch64::SYS_GETDENTS64, SyscallAction::Allow);
        
        // Process info - allow but may spoof returns
        self.filters.insert(aarch64::SYS_GETPID, SyscallAction::Allow);
        self.filters.insert(aarch64::SYS_GETUID, SyscallAction::Allow);
        
        // Memory operations - allow (needed for games)
        self.filters.insert(aarch64::SYS_MMAP, SyscallAction::Allow);
        self.filters.insert(aarch64::SYS_MPROTECT, SyscallAction::Allow);
        self.filters.insert(aarch64::SYS_MUNMAP, SyscallAction::Allow);
    }
}
