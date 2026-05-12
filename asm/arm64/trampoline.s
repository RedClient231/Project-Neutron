// Neutron Space — ARM64 Assembly Trampolines
// Pure assembly routines for syscall interception and hooking.
// These are called from the Rust engine when inline asm is insufficient.

.global neutron_syscall_trampoline
.global neutron_hook_entry
.global neutron_hook_exit
.global neutron_context_save
.global neutron_context_restore

.text
.align 4

// ============================================================================
// neutron_syscall_trampoline
// 
// Direct syscall invocation bypassing libc.
// Parameters: x0=nr, x1=a0, x2=a1, x3=a2, x4=a3, x5=a4, x6=a5
// Returns: x0=result
// ============================================================================
neutron_syscall_trampoline:
    // Move syscall number to x8
    mov     x8, x0
    // Shift arguments: a0-a5 from x1-x6 to x0-x5
    mov     x0, x1
    mov     x1, x2
    mov     x2, x3
    mov     x3, x4
    mov     x4, x5
    mov     x5, x6
    // Invoke kernel
    svc     #0
    // Result is already in x0
    ret

// ============================================================================
// neutron_hook_entry
//
// Function hook entry point. Saves all registers, calls the Rust handler,
// then restores registers and jumps to the original function.
//
// x16 = address of HookContext struct
// x17 = address of original function (after trampoline)
// ============================================================================
neutron_hook_entry:
    // Save all callee-saved and caller-saved registers
    stp     x29, x30, [sp, #-16]!   // Frame pointer + link register
    stp     x0, x1, [sp, #-16]!
    stp     x2, x3, [sp, #-16]!
    stp     x4, x5, [sp, #-16]!
    stp     x6, x7, [sp, #-16]!
    stp     x8, x9, [sp, #-16]!
    stp     x10, x11, [sp, #-16]!
    stp     x12, x13, [sp, #-16]!
    stp     x14, x15, [sp, #-16]!
    stp     x16, x17, [sp, #-16]!
    stp     x18, x19, [sp, #-16]!
    stp     x20, x21, [sp, #-16]!
    stp     x22, x23, [sp, #-16]!
    stp     x24, x25, [sp, #-16]!
    stp     x26, x27, [sp, #-16]!
    stp     x28, x29, [sp, #-16]!

    // Save NEON/FP registers (d0-d7 used for float args)
    stp     d0, d1, [sp, #-16]!
    stp     d2, d3, [sp, #-16]!
    stp     d4, d5, [sp, #-16]!
    stp     d6, d7, [sp, #-16]!

    // Call Rust hook handler
    // x0 = pointer to saved context on stack
    mov     x0, sp
    // x1 = hook context pointer (was in x16)
    ldr     x1, [sp, #(16*4 + 16*16)]  // offset to saved x16
    bl      neutron_rust_hook_handler

    // Restore NEON registers
    ldp     d6, d7, [sp], #16
    ldp     d4, d5, [sp], #16
    ldp     d2, d3, [sp], #16
    ldp     d0, d1, [sp], #16

    // Restore general registers
    ldp     x28, x29, [sp], #16
    ldp     x26, x27, [sp], #16
    ldp     x24, x25, [sp], #16
    ldp     x22, x23, [sp], #16
    ldp     x20, x21, [sp], #16
    ldp     x18, x19, [sp], #16
    ldp     x16, x17, [sp], #16
    ldp     x14, x15, [sp], #16
    ldp     x12, x13, [sp], #16
    ldp     x10, x11, [sp], #16
    ldp     x8, x9, [sp], #16
    ldp     x6, x7, [sp], #16
    ldp     x4, x5, [sp], #16
    ldp     x2, x3, [sp], #16
    ldp     x0, x1, [sp], #16
    ldp     x29, x30, [sp], #16

    // Jump to original function
    br      x17

// ============================================================================
// neutron_hook_exit
//
// Hook exit — captures return value and optionally modifies it.
// ============================================================================
neutron_hook_exit:
    // Save return value
    stp     x29, x30, [sp, #-16]!
    stp     x0, x1, [sp, #-16]!

    // Call Rust exit handler with return value
    // x0 = original return value (already there)
    bl      neutron_rust_hook_exit_handler
    // New return value comes back in x0, save it
    mov     x9, x0

    // Restore
    ldp     x0, x1, [sp], #16
    ldp     x29, x30, [sp], #16

    // Use modified return value if needed
    mov     x0, x9
    ret

// ============================================================================
// neutron_context_save
//
// Save full CPU context for process state capture.
// x0 = pointer to context buffer (must be >= 512 bytes)
// ============================================================================
neutron_context_save:
    stp     x0, x1, [x0, #0]
    stp     x2, x3, [x0, #16]
    stp     x4, x5, [x0, #32]
    stp     x6, x7, [x0, #48]
    stp     x8, x9, [x0, #64]
    stp     x10, x11, [x0, #80]
    stp     x12, x13, [x0, #96]
    stp     x14, x15, [x0, #112]
    stp     x16, x17, [x0, #128]
    stp     x18, x19, [x0, #144]
    stp     x20, x21, [x0, #160]
    stp     x22, x23, [x0, #176]
    stp     x24, x25, [x0, #192]
    stp     x26, x27, [x0, #208]
    stp     x28, x29, [x0, #224]
    mov     x1, sp
    stp     x30, x1, [x0, #240]      // LR + SP
    // Save PC (return address)
    adr     x1, .
    str     x1, [x0, #256]
    // Save PSTATE
    mrs     x1, nzcv
    str     x1, [x0, #264]
    ret

// ============================================================================
// neutron_context_restore
//
// Restore full CPU context.
// x0 = pointer to context buffer
// ============================================================================
neutron_context_restore:
    ldp     x2, x3, [x0, #16]
    ldp     x4, x5, [x0, #32]
    ldp     x6, x7, [x0, #48]
    ldp     x8, x9, [x0, #64]
    ldp     x10, x11, [x0, #80]
    ldp     x12, x13, [x0, #96]
    ldp     x14, x15, [x0, #112]
    ldp     x16, x17, [x0, #128]
    ldp     x18, x19, [x0, #144]
    ldp     x20, x21, [x0, #160]
    ldp     x22, x23, [x0, #176]
    ldp     x24, x25, [x0, #192]
    ldp     x26, x27, [x0, #208]
    ldp     x28, x29, [x0, #224]
    ldp     x30, x1, [x0, #240]
    mov     sp, x1
    // Restore PSTATE
    ldr     x1, [x0, #264]
    msr     nzcv, x1
    // Restore x0, x1 last
    ldp     x0, x1, [x0, #0]
    ret
