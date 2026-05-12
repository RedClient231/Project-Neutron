//! Platform-specific constants and detection for Android ARM64/ARM32.

/// Android system call numbers for AArch64 (ARM64).
pub mod aarch64 {
    pub const SYS_READ: u64 = 63;
    pub const SYS_WRITE: u64 = 64;
    pub const SYS_OPENAT: u64 = 56;
    pub const SYS_CLOSE: u64 = 57;
    pub const SYS_FSTAT: u64 = 80;
    pub const SYS_MMAP: u64 = 222;
    pub const SYS_MPROTECT: u64 = 226;
    pub const SYS_MUNMAP: u64 = 215;
    pub const SYS_BRK: u64 = 214;
    pub const SYS_IOCTL: u64 = 29;
    pub const SYS_PTRACE: u64 = 117;
    pub const SYS_CLONE: u64 = 220;
    pub const SYS_EXECVE: u64 = 221;
    pub const SYS_EXIT: u64 = 93;
    pub const SYS_EXIT_GROUP: u64 = 94;
    pub const SYS_WAIT4: u64 = 260;
    pub const SYS_KILL: u64 = 129;
    pub const SYS_GETPID: u64 = 172;
    pub const SYS_GETUID: u64 = 174;
    pub const SYS_GETTID: u64 = 178;
    pub const SYS_PRCTL: u64 = 167;
    pub const SYS_PROCESS_VM_READV: u64 = 270;
    pub const SYS_PROCESS_VM_WRITEV: u64 = 271;
    pub const SYS_READLINKAT: u64 = 78;
    pub const SYS_STATFS: u64 = 43;
    pub const SYS_FSTATAT: u64 = 79;
    pub const SYS_GETDENTS64: u64 = 61;
    pub const SYS_SOCKET: u64 = 198;
    pub const SYS_CONNECT: u64 = 203;
    pub const SYS_BIND: u64 = 200;
}

/// Android system call numbers for ARM32.
pub mod arm32 {
    pub const SYS_READ: u32 = 3;
    pub const SYS_WRITE: u32 = 4;
    pub const SYS_OPEN: u32 = 5;
    pub const SYS_CLOSE: u32 = 6;
    pub const SYS_MMAP2: u32 = 192;
    pub const SYS_MPROTECT: u32 = 125;
    pub const SYS_MUNMAP: u32 = 91;
    pub const SYS_BRK: u32 = 45;
    pub const SYS_IOCTL: u32 = 54;
    pub const SYS_PTRACE: u32 = 26;
    pub const SYS_CLONE: u32 = 120;
    pub const SYS_EXECVE: u32 = 11;
    pub const SYS_EXIT: u32 = 1;
    pub const SYS_EXIT_GROUP: u32 = 248;
    pub const SYS_WAIT4: u32 = 114;
    pub const SYS_KILL: u32 = 37;
    pub const SYS_GETPID: u32 = 20;
    pub const SYS_GETUID32: u32 = 199;
    pub const SYS_GETTID: u32 = 224;
    pub const SYS_PRCTL: u32 = 172;
    pub const SYS_PROCESS_VM_READV: u32 = 376;
    pub const SYS_PROCESS_VM_WRITEV: u32 = 377;
    pub const SYS_OPENAT: u32 = 322;
}

/// Ptrace request constants.
pub mod ptrace_consts {
    pub const PTRACE_TRACEME: i64 = 0;
    pub const PTRACE_PEEKTEXT: i64 = 1;
    pub const PTRACE_PEEKDATA: i64 = 2;
    pub const PTRACE_POKETEXT: i64 = 4;
    pub const PTRACE_POKEDATA: i64 = 5;
    pub const PTRACE_CONT: i64 = 7;
    pub const PTRACE_KILL: i64 = 8;
    pub const PTRACE_SINGLESTEP: i64 = 9;
    pub const PTRACE_ATTACH: i64 = 16;
    pub const PTRACE_DETACH: i64 = 17;
    pub const PTRACE_SYSCALL: i64 = 24;
    pub const PTRACE_SETOPTIONS: i64 = 0x4200;
    pub const PTRACE_GETEVENTMSG: i64 = 0x4201;
    pub const PTRACE_GETREGS: i64 = 12;
    pub const PTRACE_SETREGS: i64 = 13;
    pub const PTRACE_GETREGSET: i64 = 0x4204;
    pub const PTRACE_SETREGSET: i64 = 0x4205;

    // Ptrace options
    pub const PTRACE_O_TRACESYSGOOD: i64 = 0x01;
    pub const PTRACE_O_TRACEFORK: i64 = 0x02;
    pub const PTRACE_O_TRACEVFORK: i64 = 0x04;
    pub const PTRACE_O_TRACECLONE: i64 = 0x08;
    pub const PTRACE_O_TRACEEXEC: i64 = 0x10;
    pub const PTRACE_O_TRACEEXIT: i64 = 0x40;
}

/// Detect the current device ABI at runtime.
pub fn detect_abi() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    { "arm64-v8a" }
    #[cfg(target_arch = "arm")]
    { "armeabi-v7a" }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "arm")))]
    { "unknown" }
}

/// Get page size for the current platform.
pub fn page_size() -> usize {
    4096
}
