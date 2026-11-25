//! Reset entry, the exception vector table and diagnostics. AArch64 only.
//!
//! The linker script (see [`crate::LINKER_SCRIPT_TEMPLATE`]) places `_start` first and
//! provides the stack-top and `.bss` symbols used here. Control passes to the program's
//! `hrx_main` (declare it with [`entry!`](crate::entry)); it must never return.
//!
//! The program must be started on core 0 **at EL3** (as after a reset over JTAG, or by a
//! first-stage boot loader set to hand over at EL3); anything else sleeps forever. What
//! startup does, in order:
//! 1. park every core but core 0; mask all exceptions;
//! 2. set EL3's control registers to known values: MMU and caches off, IRQ, FIQ and system
//!    errors taken to EL3, floating point and SIMD not trapped;
//! 3. set `CPUECTLR_EL1.SMPEN`, which the Cortex-A53 requires before its caches are used;
//! 4. point `VBAR_EL3` at the vector table;
//! 5. give exceptions their own stack (`SP_EL3`) and the program the main one (`SP_EL0`);
//! 6. zero `.bss`;
//! 7. call `hrx_main`.
//!
//! Everything runs at EL3: there is no firmware and no lower level. Interrupts stay masked
//! until the program calls [`crate::cpu::enable_irq`].

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text._start,"ax"
    .global _start
_start:
    mrs     x0, mpidr_el1
    and     x0, x0, #0xFF
    cbnz    x0, hrx_park
    mrs     x0, CurrentEL
    cmp     x0, #(3 << 2)
    b.ne    hrx_park

    msr     daifset, #0xF
    // SCTLR_EL3: the reserved-one bits; MMU, caches and alignment checks off.
    ldr     x0, =0x30C50830
    msr     sctlr_el3, x0
    // SCR_EL3: IRQ, FIQ and external aborts to EL3; AArch64 below; reserved-one bits.
    mov     x0, #0x43E
    msr     scr_el3, x0
    msr     cptr_el3, xzr
    // CPUECTLR_EL1.SMPEN.
    mrs     x0, S3_1_C15_C2_1
    orr     x0, x0, #(1 << 6)
    msr     S3_1_C15_C2_1, x0
    isb
    ldr     x0, =hrx_vectors
    msr     vbar_el3, x0

    msr     spsel, #1
    ldr     x0, =__stack_exc_top
    mov     sp, x0
    msr     spsel, #0
    ldr     x0, =__stack_main_top
    mov     sp, x0
    isb

    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
1:  cmp     x0, x1
    b.hs    2f
    str     xzr, [x0], #8
    b       1b
2:  bl      hrx_main
    // hrx_main must not return; if it does, stop.
    .global hrx_park
hrx_park:
    wfe
    b       hrx_park

    // ---- vectors: 16 entries of 128 bytes, the table 2 KiB aligned ----
    .macro  VECTOR target
    .balign 128
    b       \target
    .endm

    .section .text.hrx_vectors,"ax"
    .balign 2048
    .global hrx_vectors
hrx_vectors:
    // from EL3 on SP_EL0: the program
    VECTOR  hrx_sync_entry
    VECTOR  hrx_irq_entry
    VECTOR  hrx_fiq_entry
    VECTOR  hrx_serror_entry
    // from EL3 on SP_EL3: inside a handler
    VECTOR  hrx_sync_entry
    VECTOR  hrx_irq_entry
    VECTOR  hrx_fiq_entry
    VECTOR  hrx_serror_entry
    // from lower levels, which nothing here uses
    .rept   8
    VECTOR  hrx_lower_entry
    .endr

    // Unhandled exceptions: save x0..x30 and hand the frame to Rust to report.
    .macro  REPORT name, kind
    .global \name
\name:
    sub     sp, sp, #256
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
    str     x30, [sp, #240]
    mov     x0, #\kind
    mov     x1, sp
    bl      hrx_exception
    b       hrx_park
    .endm

    .section .text.hrx_exceptions,"ax"
    REPORT  hrx_fiq_entry, 2
    REPORT  hrx_serror_entry, 3
    REPORT  hrx_lower_entry, 4
"#
);

#[cfg(feature = "default-sync-entry")]
global_asm!(
    r#"
    .section .text.hrx_exceptions,"ax"
    REPORT  hrx_sync_entry, 1
"#
);

// The handler is ordinary Rust, so everything a call may change is saved: x0..x18, x29, x30,
// the return state, the floating-point control and status registers, and all of q0..q31 (a
// callee keeps only the lower halves of v8..v15).
#[cfg(feature = "default-irq-entry")]
global_asm!(
    r#"
    .section .text.hrx_exceptions,"ax"
    .global hrx_irq_entry
hrx_irq_entry:
    sub     sp, sp, #720
    stp     x0, x1, [sp, #0]
    stp     x2, x3, [sp, #16]
    stp     x4, x5, [sp, #32]
    stp     x6, x7, [sp, #48]
    stp     x8, x9, [sp, #64]
    stp     x10, x11, [sp, #80]
    stp     x12, x13, [sp, #96]
    stp     x14, x15, [sp, #112]
    stp     x16, x17, [sp, #128]
    stp     x18, x29, [sp, #144]
    mrs     x0, elr_el3
    mrs     x1, spsr_el3
    stp     x30, x0, [sp, #160]
    mrs     x2, fpcr
    mrs     x3, fpsr
    stp     x1, x2, [sp, #176]
    str     x3, [sp, #192]
    stp     q0, q1, [sp, #208]
    stp     q2, q3, [sp, #240]
    stp     q4, q5, [sp, #272]
    stp     q6, q7, [sp, #304]
    stp     q8, q9, [sp, #336]
    stp     q10, q11, [sp, #368]
    stp     q12, q13, [sp, #400]
    stp     q14, q15, [sp, #432]
    stp     q16, q17, [sp, #464]
    stp     q18, q19, [sp, #496]
    stp     q20, q21, [sp, #528]
    stp     q22, q23, [sp, #560]
    stp     q24, q25, [sp, #592]
    stp     q26, q27, [sp, #624]
    stp     q28, q29, [sp, #656]
    stp     q30, q31, [sp, #688]
    bl      hrx_irq_dispatch
    ldp     q0, q1, [sp, #208]
    ldp     q2, q3, [sp, #240]
    ldp     q4, q5, [sp, #272]
    ldp     q6, q7, [sp, #304]
    ldp     q8, q9, [sp, #336]
    ldp     q10, q11, [sp, #368]
    ldp     q12, q13, [sp, #400]
    ldp     q14, q15, [sp, #432]
    ldp     q16, q17, [sp, #464]
    ldp     q18, q19, [sp, #496]
    ldp     q20, q21, [sp, #528]
    ldp     q22, q23, [sp, #560]
    ldp     q24, q25, [sp, #592]
    ldp     q26, q27, [sp, #624]
    ldp     q28, q29, [sp, #656]
    ldp     q30, q31, [sp, #688]
    ldr     x3, [sp, #192]
    ldp     x1, x2, [sp, #176]
    msr     fpsr, x3
    msr     fpcr, x2
    ldp     x30, x0, [sp, #160]
    msr     elr_el3, x0
    msr     spsr_el3, x1
    ldp     x18, x29, [sp, #144]
    ldp     x16, x17, [sp, #128]
    ldp     x14, x15, [sp, #112]
    ldp     x12, x13, [sp, #96]
    ldp     x10, x11, [sp, #80]
    ldp     x8, x9, [sp, #64]
    ldp     x6, x7, [sp, #48]
    ldp     x4, x5, [sp, #32]
    ldp     x2, x3, [sp, #16]
    ldp     x0, x1, [sp, #0]
    add     sp, sp, #720
    eret
"#
);

/// Called by the IRQ entry: acknowledge the interrupt, run its handler, signal the end.
#[cfg(feature = "default-irq-entry")]
#[unsafe(no_mangle)]
extern "C" fn hrx_irq_dispatch() {
    use hardroxide_mmio::{MmioBackend, PhysMem};
    // SAFETY: the GIC windows belong to this crate's interrupt handling alone.
    let be = unsafe { PhysMem::new() };
    let (Ok(d), Ok(c)) = (
        be.map(crate::map::GIC_DIST_BASE as u64, crate::map::GIC_DIST_SIZE),
        be.map(crate::map::GIC_CPU_BASE as u64, crate::map::GIC_CPU_SIZE),
    ) else {
        return;
    };
    let gic = crate::gic::Gic::new(d, c);
    if let Ok(Some(id)) = gic.acknowledge() {
        // An interrupt in event mode, or with no handler, is masked until something asks
        // for it again (a level-triggered line would otherwise fire continuously).
        if crate::irq::dispatch(id) != crate::irq::Dispatched::Handler {
            let _ = gic.disable(id);
        }
        let _ = gic.end_of_interrupt(id);
    }
}

/// Report an unhandled exception on the console and halt. `regs` points at the saved
/// `x0..x30` (31 words).
#[unsafe(no_mangle)]
extern "C" fn hrx_exception(kind: u64, regs: *const u64) -> ! {
    use crate::cpu;
    let esr = cpu::esr();
    let name = match (kind, esr >> 26) {
        (2, _) => "fast interrupt",
        (3, _) => "system error",
        (4, _) => "exception from a lower level",
        (_, 0x00) => "undefined instruction",
        (_, 0x07) => "floating point or SIMD trapped",
        (_, 0x15 | 0x17) => "supervisor or monitor call",
        (_, 0x20 | 0x21) => "instruction abort",
        (_, 0x22) => "misaligned program counter",
        (_, 0x24 | 0x25) => "data abort",
        (_, 0x26) => "misaligned stack pointer",
        (_, 0x3C) => "breakpoint instruction",
        _ => "synchronous exception",
    };
    crate::println!("\n*** EXCEPTION: {name}");
    crate::print!("  ELR = {:#018x}  ESR = {esr:#010x}", cpu::elr());
    // The fault address means something only for memory faults and a misaligned PC.
    if kind == 1 && matches!(esr >> 26, 0x20 | 0x21 | 0x22 | 0x24 | 0x25) {
        crate::print!("  FAR = {:#018x}", cpu::far());
    }
    crate::println!();
    if !regs.is_null() {
        // SAFETY: the entry stub stored exactly 31 words at `regs`.
        let r = unsafe { core::slice::from_raw_parts(regs, 31) };
        for (i, v) in r.iter().enumerate() {
            crate::print!("  x{i:<2} = {v:#018x}");
            if i % 3 == 2 {
                crate::println!();
            }
        }
        crate::println!();
    }
    loop {
        cpu::wfe();
    }
}

/// Declare the program's entry function. It runs on core 0 at EL3 after
/// [`start`](self); it must not return.
///
/// ```ignore
/// hardroxide_psu::entry!(main);
/// fn main() -> ! { /* ... */ loop {} }
/// ```
#[macro_export]
macro_rules! entry {
    ($path:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn hrx_main() -> ! {
            let f: fn() -> ! = $path;
            f()
        }
    };
}
