//! Raw syscall wrappers using inline assembly for AArch64 and ARM32.
//! 
//! These bypass libc entirely for critical operations where we need
//! precise control over syscall arguments and return values.

/// Execute a raw syscall with 0-6 arguments on AArch64.
/// Uses inline assembly to directly invoke the kernel.
#[cfg(target_arch = "aarch64")]
pub mod raw {
    /// Raw syscall with no arguments.
    #[inline(always)]
    pub unsafe fn syscall0(nr: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            lateout("x0") ret,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 1 argument.
    #[inline(always)]
    pub unsafe fn syscall1(nr: u64, a0: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 2 arguments.
    #[inline(always)]
    pub unsafe fn syscall2(nr: u64, a0: u64, a1: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 3 arguments.
    #[inline(always)]
    pub unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 4 arguments.
    #[inline(always)]
    pub unsafe fn syscall4(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 5 arguments.
    #[inline(always)]
    pub unsafe fn syscall5(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 6 arguments.
    #[inline(always)]
    pub unsafe fn syscall6(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
        let ret: i64;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack)
        );
        ret
    }

    /// Raw ptrace syscall — specialized for precision.
    #[inline(always)]
    pub unsafe fn ptrace_raw(request: u64, pid: u64, addr: u64, data: u64) -> i64 {
        syscall4(super::super::platform::aarch64::SYS_PTRACE, request, pid, addr, data)
    }

    /// Raw process_vm_readv — read memory from another process.
    #[inline(always)]
    pub unsafe fn process_vm_readv(
        pid: u64,
        local_iov: u64,
        local_iovcnt: u64,
        remote_iov: u64,
        remote_iovcnt: u64,
        flags: u64,
    ) -> i64 {
        syscall6(
            super::super::platform::aarch64::SYS_PROCESS_VM_READV,
            pid, local_iov, local_iovcnt, remote_iov, remote_iovcnt, flags,
        )
    }

    /// Raw process_vm_writev — write memory to another process.
    #[inline(always)]
    pub unsafe fn process_vm_writev(
        pid: u64,
        local_iov: u64,
        local_iovcnt: u64,
        remote_iov: u64,
        remote_iovcnt: u64,
        flags: u64,
    ) -> i64 {
        syscall6(
            super::super::platform::aarch64::SYS_PROCESS_VM_WRITEV,
            pid, local_iov, local_iovcnt, remote_iov, remote_iovcnt, flags,
        )
    }
}

/// Execute raw syscalls on ARM32 using inline assembly.
#[cfg(target_arch = "arm")]
pub mod raw {
    /// Raw syscall with no arguments (ARM32).
    #[inline(always)]
    pub unsafe fn syscall0(nr: u32) -> i32 {
        let ret: i32;
        core::arch::asm!(
            "svc #0",
            in("r7") nr,
            lateout("r0") ret,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 1 argument (ARM32).
    #[inline(always)]
    pub unsafe fn syscall1(nr: u32, a0: u32) -> i32 {
        let ret: i32;
        core::arch::asm!(
            "svc #0",
            in("r7") nr,
            inlateout("r0") a0 => ret,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 2 arguments (ARM32).
    #[inline(always)]
    pub unsafe fn syscall2(nr: u32, a0: u32, a1: u32) -> i32 {
        let ret: i32;
        core::arch::asm!(
            "svc #0",
            in("r7") nr,
            inlateout("r0") a0 => ret,
            in("r1") a1,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 3 arguments (ARM32).
    #[inline(always)]
    pub unsafe fn syscall3(nr: u32, a0: u32, a1: u32, a2: u32) -> i32 {
        let ret: i32;
        core::arch::asm!(
            "svc #0",
            in("r7") nr,
            inlateout("r0") a0 => ret,
            in("r1") a1,
            in("r2") a2,
            options(nostack)
        );
        ret
    }

    /// Raw syscall with 4 arguments (ARM32).
    #[inline(always)]
    pub unsafe fn syscall4(nr: u32, a0: u32, a1: u32, a2: u32, a3: u32) -> i32 {
        let ret: i32;
        core::arch::asm!(
            "svc #0",
            in("r7") nr,
            inlateout("r0") a0 => ret,
            in("r1") a1,
            in("r2") a2,
            in("r3") a3,
            options(nostack)
        );
        ret
    }

    /// Raw ptrace syscall (ARM32).
    #[inline(always)]
    pub unsafe fn ptrace_raw(request: u32, pid: u32, addr: u32, data: u32) -> i32 {
        syscall4(super::super::platform::arm32::SYS_PTRACE, request, pid, addr, data)
    }
}

/// Fallback for non-ARM targets (for testing/CI on x86_64).
#[cfg(not(any(target_arch = "aarch64", target_arch = "arm")))]
pub mod raw {
    #[inline(always)]
    pub unsafe fn syscall0(_nr: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn syscall1(_nr: u64, _a0: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn syscall2(_nr: u64, _a0: u64, _a1: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn syscall3(_nr: u64, _a0: u64, _a1: u64, _a2: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn syscall4(_nr: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn syscall5(_nr: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn syscall6(_nr: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn ptrace_raw(_request: u64, _pid: u64, _addr: u64, _data: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn process_vm_readv(_pid: u64, _local_iov: u64, _local_iovcnt: u64, _remote_iov: u64, _remote_iovcnt: u64, _flags: u64) -> i64 { -1 }
    #[inline(always)]
    pub unsafe fn process_vm_writev(_pid: u64, _local_iov: u64, _local_iovcnt: u64, _remote_iov: u64, _remote_iovcnt: u64, _flags: u64) -> i64 { -1 }
}
