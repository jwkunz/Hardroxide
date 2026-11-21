//! The Cortex-A9 port: context switching. ARM only.
//!
//! A task's context lives on its own (System-mode) stack while it is not running:
//!
//! ```text
//! low   d0-d15 (32 words)   fpscr + pad (2)   r0-r12, lr (14)   pc, cpsr (2)   high
//! ```
//!
//! Both entries (an interrupt, and the `svc` that a blocking call executes) push the
//! interrupted task's context this way with `srs`, hand the stack pointer to Rust, which saves
//! it in the task's control block, picks the next task and returns *its* stack pointer, and
//! restore from that stack with `rfe`. Interrupts stay masked in the kernel; a task runs in
//! System mode with them enabled.

use core::arch::global_asm;

/// Words in a saved context.
pub const FRAME_WORDS: usize = 50;
/// System mode, IRQ and FIQ enabled, ARM state.
pub const TASK_CPSR: u32 = 0x1F;

global_asm!(
    r#"
    .arm
    .fpu vfpv3-d16
    .section .text.hrx_rt_port,"ax"

    @ Save the interrupted context onto the System-mode stack (already switched to).
    .macro SAVE_CONTEXT
    push    {{r0-r12, lr}}
    vmrs    r1, fpscr
    push    {{r1, r2}}
    vpush   {{d0-d15}}
    .endm

    .macro RESTORE_CONTEXT
    vpop    {{d0-d15}}
    pop     {{r1, r2}}
    vmsr    fpscr, r1
    pop     {{r0-r12, lr}}
    rfeia   sp!
    .endm

    .global hrx_irq_entry
hrx_irq_entry:
    sub     lr, lr, #4
    srsdb   sp!, #0x1F              @ push the return address and SPSR onto the System stack
    cps     #0x1F
    SAVE_CONTEXT
    mov     r0, sp
    cps     #0x12                   @ run the Rust handler on the IRQ stack
    bl      hrx_rt_irq
    cps     #0x1F
    mov     sp, r0
    RESTORE_CONTEXT

    .global hrx_svc_entry
hrx_svc_entry:
    srsdb   sp!, #0x1F
    cps     #0x1F
    SAVE_CONTEXT
    mov     r0, sp
    cps     #0x13
    bl      hrx_rt_svc
    cps     #0x1F
    mov     sp, r0
    RESTORE_CONTEXT

    @ Start the first task: r0 = its saved stack pointer.
    .global hrx_rt_start_first
hrx_rt_start_first:
    mov     sp, r0
    RESTORE_CONTEXT
"#
);

unsafe extern "C" {
    /// Begin running the task whose saved context is at `sp`. Never returns.
    pub fn hrx_rt_start_first(sp: usize) -> !;
}

/// Block the calling task and let the scheduler choose (a supervisor call).
#[inline(always)]
pub fn yield_now() {
    // SAFETY: the SVC entry saves and restores the whole task context.
    unsafe {
        core::arch::asm!("svc #0", out("r0") _, out("r1") _, out("r2") _, out("r3") _, out("r12") _, options(nostack))
    };
}

/// Build the initial saved context for a task at the top of `stack`, so that "restoring" it
/// starts `entry`; when `entry` returns it continues at `on_return`. Returns the stack
/// pointer to store in the task's control block.
pub fn init_frame(stack: &'static mut [u8], entry: fn(), on_return: extern "C" fn() -> !) -> usize {
    let top = (stack.as_mut_ptr() as usize + stack.len()) & !7;
    let sp = top - FRAME_WORDS * 4;
    let frame = sp as *mut u32;
    // SAFETY: `sp..top` lies inside `stack` (checked by the caller via its length), which this
    // function has exclusive access to.
    unsafe {
        for i in 0..FRAME_WORDS {
            frame.add(i).write(0);
        }
        // r0-r12 are words 34..47, lr is 47, pc 48, cpsr 49.
        frame.add(47).write(on_return as usize as u32);
        frame.add(48).write(entry as usize as u32);
        frame.add(49).write(TASK_CPSR);
    }
    sp
}
