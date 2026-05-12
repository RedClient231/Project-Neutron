@ Neutron Space — ARM32 Assembly Trampolines
@ For 32-bit game compatibility on ARM32 (armeabi-v7a)

.global neutron_syscall_trampoline_arm32
.global neutron_hook_entry_arm32
.global neutron_context_save_arm32
.global neutron_context_restore_arm32

.text
.align 4
.arm

@ ============================================================================
@ neutron_syscall_trampoline_arm32
@
@ Direct syscall invocation for ARM32.
@ Parameters: r0=nr, r1=a0, r2=a1, r3=a2, [sp]=a3, [sp+4]=a4, [sp+8]=a5
@ Returns: r0=result
@ ============================================================================
neutron_syscall_trampoline_arm32:
    push    {r4-r7, lr}
    @ Move syscall number to r7
    mov     r7, r0
    @ Shift arguments
    mov     r0, r1
    mov     r1, r2
    mov     r2, r3
    @ Load remaining args from stack (offset by pushed registers)
    ldr     r3, [sp, #20]       @ a3
    ldr     r4, [sp, #24]       @ a4
    ldr     r5, [sp, #28]       @ a5
    @ Invoke kernel
    svc     #0
    @ Result in r0
    pop     {r4-r7, pc}

@ ============================================================================
@ neutron_hook_entry_arm32
@
@ Function hook entry for ARM32.
@ Saves context, calls Rust handler, restores, jumps to original.
@ r12 = original function address
@ ============================================================================
neutron_hook_entry_arm32:
    @ Save all registers
    push    {r0-r12, lr}
    @ Save VFP registers (d0-d7 for float args)
    vpush   {d0-d7}

    @ Call Rust handler
    @ r0 = stack pointer (context)
    mov     r0, sp
    bl      neutron_rust_hook_handler_arm32

    @ Restore VFP
    vpop    {d0-d7}
    @ Restore general registers
    pop     {r0-r12, lr}

    @ Jump to original function (address in r12)
    bx      r12

@ ============================================================================
@ neutron_context_save_arm32
@
@ Save full ARM32 CPU context.
@ r0 = pointer to context buffer (>= 256 bytes)
@ ============================================================================
neutron_context_save_arm32:
    @ Save r0-r12
    stm     r0, {r0-r12}
    @ Save sp, lr, pc
    str     sp, [r0, #52]
    str     lr, [r0, #56]
    @ Save CPSR
    mrs     r1, cpsr
    str     r1, [r0, #60]
    @ Return
    bx      lr

@ ============================================================================
@ neutron_context_restore_arm32
@
@ Restore full ARM32 CPU context.
@ r0 = pointer to context buffer
@ ============================================================================
neutron_context_restore_arm32:
    @ Restore CPSR first
    ldr     r1, [r0, #60]
    msr     cpsr_f, r1
    @ Restore sp, lr
    ldr     sp, [r0, #52]
    ldr     lr, [r0, #56]
    @ Restore r0-r12
    ldm     r0, {r0-r12}
    bx      lr
