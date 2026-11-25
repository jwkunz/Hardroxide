//! The Cortex-A53 port: context switching, at EL3. AArch64 only.
//!
//! Tasks run at EL3 on `SP_EL0`; the kernel's handlers run on `SP_EL3`, the exception stack of
//! the linker script. A task's context lives on its own stack while it is not running:
//!
//! ```text
//! low   x0-x30 (31 words)   pc, spsr, fpcr, fpsr   pad (1)   q0-q31 (64 words)   high
//! ```
//!
//! The whole vector file is saved: the calling convention keeps only half of `v8`..`v15`
//! across calls, and a preempted task may be anywhere.
//!
//! Both entries (an interrupt, and the `svc` that a blocking call executes) push the
//! interrupted task's context this way, hand its stack pointer to Rust, which saves it in the
//! task's control block, picks the next task and returns *its* stack pointer, and restore
//! from that stack with `eret`. Interrupts stay masked in the kernel; a task runs with them
//! enabled. Any other synchronous exception, including `svc` from inside a handler, is
//! reported on the console as without a kernel.

use core::arch::global_asm;

/// What the kernel needs from the processor besides the context switch.
pub(crate) mod platform {
    pub use hardroxide_psu::cpu::{wfe, wfi, without_irq};
    pub use hardroxide_psu::{gic::Gic, irq, map};

    use hardroxide_mmio::PhysMem;
    use hardroxide_psu::timer;

    /// The interrupt of the tick timer.
    pub const TICK_INTERRUPT: u32 = map::ppi::SECURE_PHYSICAL_TIMER;
    /// The smallest task stack accepted: a saved context takes 800 bytes of it.
    pub const MIN_STACK: usize = 2048;
    /// The idle task's stack.
    pub const IDLE_STACK: usize = 4096;

    /// The system counter's low word: it wraps every two minutes at 33 MHz (intervals between uses are far shorter).
    #[inline(always)]
    pub fn stamp() -> u32 {
        timer::counter() as u32
    }

    /// Start the secure physical timer interrupting `tick_hz` times a second; `counter_hz` is the system counter's clock.
    pub fn start_tick(_be: &PhysMem, counter_hz: u64, tick_hz: u32) {
        timer::start_periodic(counter_hz / u64::from(tick_hz.max(1)));
    }

    /// In the tick's interrupt: arm the next period (which drops the timer's line).
    pub fn acknowledge_tick(_be: &PhysMem) {
        timer::tick();
    }
}

/// Words (64 bits each) in a saved context.
pub const FRAME_WORDS: usize = 100;
/// EL3 on `SP_EL0`, all interrupts enabled.
pub const TASK_SPSR: u64 = 0b1100;

// Frame offsets, in words: x30, the return address and the saved status.
const X30: usize = 30;
const PC: usize = 31;
const SPSR: usize = 32;

global_asm!(
    r#"
    .section .text.hrx_rt_port,"ax"

    // Save the interrupted context onto the task's stack (SP_EL0, already selected).
    .macro SAVE_CONTEXT
    sub     sp, sp, #800
    stp     x0, x1, [sp, #0]
    stp     x2, x3, [sp, #16]
    stp     x4, x5, [sp, #32]
    stp     x6, x7, [sp, #48]
    stp     x8, x9, [sp, #64]
    stp     x10, x11, [sp, #80]
    stp     x12, x13, [sp, #96]
    stp     x14, x15, [sp, #112]
    stp     x16, x17, [sp, #128]
    stp     x18, x19, [sp, #144]
    stp     x20, x21, [sp, #160]
    stp     x22, x23, [sp, #176]
    stp     x24, x25, [sp, #192]
    stp     x26, x27, [sp, #208]
    stp     x28, x29, [sp, #224]
    mrs     x0, elr_el3
    stp     x30, x0, [sp, #240]
    mrs     x0, spsr_el3
    mrs     x1, fpcr
    stp     x0, x1, [sp, #256]
    mrs     x0, fpsr
    str     x0, [sp, #272]
    stp     q0, q1, [sp, #288]
    stp     q2, q3, [sp, #320]
    stp     q4, q5, [sp, #352]
    stp     q6, q7, [sp, #384]
    stp     q8, q9, [sp, #416]
    stp     q10, q11, [sp, #448]
    stp     q12, q13, [sp, #480]
    stp     q14, q15, [sp, #512]
    stp     q16, q17, [sp, #544]
    stp     q18, q19, [sp, #576]
    stp     q20, q21, [sp, #608]
    stp     q22, q23, [sp, #640]
    stp     q24, q25, [sp, #672]
    stp     q26, q27, [sp, #704]
    stp     q28, q29, [sp, #736]
    stp     q30, q31, [sp, #768]
    .endm

    // Restore from the task's stack (SP_EL0, selected) and resume it. Interrupts must be
    // masked: the return state is held in ELR_EL3 and SPSR_EL3 until `eret`.
    .macro RESTORE_CONTEXT
    ldp     q0, q1, [sp, #288]
    ldp     q2, q3, [sp, #320]
    ldp     q4, q5, [sp, #352]
    ldp     q6, q7, [sp, #384]
    ldp     q8, q9, [sp, #416]
    ldp     q10, q11, [sp, #448]
    ldp     q12, q13, [sp, #480]
    ldp     q14, q15, [sp, #512]
    ldp     q16, q17, [sp, #544]
    ldp     q18, q19, [sp, #576]
    ldp     q20, q21, [sp, #608]
    ldp     q22, q23, [sp, #640]
    ldp     q24, q25, [sp, #672]
    ldp     q26, q27, [sp, #704]
    ldp     q28, q29, [sp, #736]
    ldp     q30, q31, [sp, #768]
    ldr     x0, [sp, #272]
    msr     fpsr, x0
    ldp     x0, x1, [sp, #256]
    msr     spsr_el3, x0
    msr     fpcr, x1
    ldp     x30, x0, [sp, #240]
    msr     elr_el3, x0
    ldp     x2, x3, [sp, #16]
    ldp     x4, x5, [sp, #32]
    ldp     x6, x7, [sp, #48]
    ldp     x8, x9, [sp, #64]
    ldp     x10, x11, [sp, #80]
    ldp     x12, x13, [sp, #96]
    ldp     x14, x15, [sp, #112]
    ldp     x16, x17, [sp, #128]
    ldp     x18, x19, [sp, #144]
    ldp     x20, x21, [sp, #160]
    ldp     x22, x23, [sp, #176]
    ldp     x24, x25, [sp, #192]
    ldp     x26, x27, [sp, #208]
    ldp     x28, x29, [sp, #224]
    ldp     x0, x1, [sp, #0]
    add     sp, sp, #800
    eret
    .endm

    // Switch contexts around `handler`, which takes the interrupted task's stack pointer
    // and returns the next one's.
    .macro SWITCH handler
    msr     spsel, #0
    SAVE_CONTEXT
    mov     x0, sp
    msr     spsel, #1
    bl      \handler
    msr     spsel, #0
    mov     sp, x0
    RESTORE_CONTEXT
    .endm

    .global hrx_irq_entry
hrx_irq_entry:
    SWITCH  hrx_rt_irq

    .global hrx_sync_entry
hrx_sync_entry:
    // A supervisor call (syndrome class 0x15) from a task, that is from SP_EL0; here on SP_EL3.
    stp     x0, x1, [sp, #-16]!
    mrs     x0, esr_el3
    lsr     x0, x0, #26
    cmp     x0, #0x15
    b.ne    1f
    mrs     x0, spsr_el3
    tbnz    x0, #0, 1f
    ldp     x0, x1, [sp], #16
    SWITCH  hrx_rt_svc
1:  ldp     x0, x1, [sp], #16
    b       hrx_report_sync

    // Start the first task: x0 = its saved stack pointer.
    .global hrx_rt_start_first
hrx_rt_start_first:
    msr     daifset, #0xF
    msr     spsel, #0
    mov     sp, x0
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
    // SAFETY: the supervisor-call entry saves and restores the whole task context (on this
    // stack, hence not `nostack`).
    unsafe { core::arch::asm!("svc #0") };
}

/// Build the initial saved context for a task at the top of `stack`, so that "restoring" it
/// starts `entry`; when `entry` returns it continues at `on_return`. Returns the stack
/// pointer to store in the task's control block.
pub fn init_frame(stack: &'static mut [u8], entry: fn(), on_return: extern "C" fn() -> !) -> usize {
    let top = (stack.as_mut_ptr() as usize + stack.len()) & !15;
    let sp = top - FRAME_WORDS * 8;
    let frame = sp as *mut u64;
    // SAFETY: `sp..top` lies inside `stack` (checked by the caller via its length), which this
    // function has exclusive access to.
    unsafe {
        for i in 0..FRAME_WORDS {
            frame.add(i).write(0);
        }
        frame.add(X30).write(on_return as usize as u64);
        frame.add(PC).write(entry as usize as u64);
        frame.add(SPSR).write(TASK_SPSR);
    }
    sp
}
