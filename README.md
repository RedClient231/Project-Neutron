# Neutron Space

A lightweight Android virtual space engine written entirely in **Rust** and **Assembly** (ARM64 + ARM32). No Java, no C++, no Kotlin.

## Features

- **Virtual App Environment** — Run any APK/XAPK in an isolated virtual space
- **No Root Required** — Works on stock Android 13+ devices
- **32-bit & 64-bit Game Support** — Full ARM32 and ARM64 native library compatibility
- **GameGuardian Compatible** — Memory editing tools work within the virtual space
- **APK/XAPK Import** — Install from file manager or clone installed apps
- **Anti-Detection** — Games cannot detect the virtual environment
- **GPU Passthrough** — Full Mali GPU acceleration (Helio G99 / Mali-G57 optimized)
- **Lightweight** — Minimal memory footprint, optimized for performance

## Architecture

```
neutron-core/      — Core types, syscall wrappers (inline ARM assembly), config
neutron-vfs/       — Virtual filesystem overlay, path redirection, /proc spoofing
neutron-engine/    — Process virtualization, ptrace tracer, hook manager, namespace
neutron-apk/       — APK/XAPK parser, installer, binary XML manifest parser
neutron-compat/    — Game compat layer, GameGuardian support, GPU, anti-detection
neutron-ui/        — Slint-based UI (pure Rust, no Java needed)
neutron-app/       — Android NativeActivity entry point, orchestration
asm/arm64/         — ARM64 assembly trampolines for hooking and syscall interception
asm/arm32/         — ARM32 assembly trampolines for 32-bit compatibility
```

## How It Works

1. **Process Isolation**: Spawns guest apps as child processes using `fork()`
2. **Syscall Interception**: Attaches via `ptrace` to intercept all syscalls
3. **Filesystem Redirection**: `openat`/`stat` calls are rewritten to virtual paths
4. **Identity Spoofing**: `/proc` entries are spoofed to hide the virtual environment
5. **Memory Access**: `process_vm_readv`/`writev` enables GameGuardian's memory editing
6. **GPU Passthrough**: Mali GPU device nodes pass through unmodified for performance

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (100% safe where possible) |
| Low-level | ARM64 & ARM32 inline assembly |
| Syscalls | Direct kernel invocation via `svc #0` |
| UI | Slint (declarative, no Java) |
| Android Entry | NativeActivity (no JVM) |
| Build | cargo-ndk + GitHub Actions |

## Building

### Prerequisites

- Rust stable (1.75+)
- Android NDK r26+
- `cargo-ndk`

### Build Commands

```bash
# ARM64
cargo ndk -t arm64-v8a -p 33 build --release -p neutron-app

# ARM32
cargo ndk -t armeabi-v7a -p 33 build --release -p neutron-app
```

### CI/CD

Push to `main` triggers automated APK build via GitHub Actions.

## Target Device

- **Android**: 13+ (API 33)
- **CPU**: MediaTek Helio G99 (ARM Cortex-A76 + A55)
- **GPU**: Mali-G57 MC2
- **Root**: Not required

## License

GPL-3.0
