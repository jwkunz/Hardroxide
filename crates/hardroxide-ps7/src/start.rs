//! Reset entry, stacks, the exception vector table and diagnostics. ARM only.
//!
//! The linker script (see [`crate::LINKER_SCRIPT_TEMPLATE`]) places `_start` first and
//! provides the stack-top and `.bss` symbols used here. Control passes to the program's
//! `hrx_main` (declare it with [`entry!`](crate::entry)); it must never return.
//!
//! What startup does, in order, on core 0 (core 1 is parked):
//! 1. enter System mode with interrupts masked and give every exception mode its own
//!    stack;
//! 2. enable the VFP so floating-point code runs;
//! 3. point `VBAR` at the vector table;
//! 4. zero `.bss`;
//! 5. call `hrx_main`.
//!
//! Interrupts stay masked until the program calls [`crate::cp15::enable_irq`].

use core::arch::global_asm;

global_asm!(
    r#"
    .arm
    .fpu vfpv3-d16

    .section .text._start,"ax"
    .global _start
_start:
    @ Only core 0 runs the program; a second core that reaches here sleeps forever.
    mrc     p15, 0, r0, c0, c0, 5
    ands    r0, r0, #3
    bne     hrx_park

    @ System mode, IRQ and FIQ masked; then a stack for each exception mode.
    msr     cpsr_c, #0xD1           @ FIQ
    ldr     sp, =__stack_fiq_top
    msr     cpsr_c, #0xD2           @ IRQ
    ldr     sp, =__stack_irq_top
    msr     cpsr_c, #0xD7           @ abort
    ldr     sp, =__stack_abt_top
    msr     cpsr_c, #0xDB           @ undefined
    ldr     sp, =__stack_und_top
    msr     cpsr_c, #0xD3           @ supervisor
    ldr     sp, =__stack_svc_top
    msr     cpsr_c, #0xDF           @ system (the program runs here)
    ldr     sp, =__stack_sys_top

    @ Enable VFP/NEON (CP10/CP11 full access, FPEXC.EN).
    mrc     p15, 0, r0, c1, c0, 2
    orr     r0, r0, #(0xF << 20)
    mcr     p15, 0, r0, c1, c0, 2
    isb
    mov     r0, #0x40000000
    vmsr    fpexc, r0

    @ Exception vectors: low vectors off, base from VBAR.
    mrc     p15, 0, r0, c1, c0, 0
    bic     r0, r0, #(1 << 13)
    mcr     p15, 0, r0, c1, c0, 0
    ldr     r0, =hrx_vectors
    mcr     p15, 0, r0, c12, c0, 0
    isb

    @ Zero .bss.
    ldr     r0, =__bss_start
    ldr     r1, =__bss_end
    mov     r2, #0
1:  cmp     r0, r1
    strlo   r2, [r0], #4
    blo     1b

    bl      hrx_main
    @ hrx_main must not return; if it does, stop.
hrx_park:
2:  wfe
    b       2b

    .section .text.hrx_vectors,"ax"
    .align  5
    .global hrx_vectors
hrx_vectors:
    b       _start
    b       hrx_undef_entry
    b       hrx_svc_entry
    b       hrx_pabt_entry
    b       hrx_dabt_entry
    b       hrx_dabt_entry          @ reserved vector
    b       hrx_irq_entry
    b       hrx_fiq_entry

    .section .text.hrx_exceptions,"ax"
    @ Unhandled-exception stubs: save everything and hand the frame to Rust to report.
    .macro  EXC_ENTRY name, kind, adjust
    .global \name
\name:
    sub     lr, lr, #\adjust
    stmfd   sp!, {{r0-r12, lr}}
    mov     r0, #\kind
    mov     r1, sp
    bl      hrx_exception
3:  wfe
    b       3b
    .endm

    EXC_ENTRY hrx_undef_entry, 1, 4
    EXC_ENTRY hrx_pabt_entry, 3, 4
    EXC_ENTRY hrx_dabt_entry, 4, 8
    EXC_ENTRY hrx_fiq_entry, 5, 4
"#
);

#[cfg(feature = "default-svc-entry")]
global_asm!(
    r#"
    .arm
    .section .text.hrx_exceptions,"ax"
    .global hrx_svc_entry
hrx_svc_entry:
    stmfd   sp!, {{r0-r12, lr}}
    mov     r0, #2
    mov     r1, sp
    bl      hrx_exception
4:  wfe
    b       4b
"#
);

#[cfg(feature = "default-irq-entry")]
global_asm!(
    r#"
    .arm
    .fpu vfpv3-d16
    .section .text.hrx_exceptions,"ax"
    .global hrx_irq_entry
hrx_irq_entry:
    sub     lr, lr, #4
    stmfd   sp!, {{r0-r3, r12, lr}}
    @ Save the caller-saved VFP state (d0-d7, FPSCR): handlers are ordinary Rust.
    vmrs    r1, fpscr
    stmfd   sp!, {{r1, r2}}
    vstmdb  sp!, {{d0-d7}}
    bl      hrx_irq_dispatch
    vldmia  sp!, {{d0-d7}}
    ldmfd   sp!, {{r1, r2}}
    vmsr    fpscr, r1
    ldmfd   sp!, {{r0-r3, r12, pc}}^
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
/// `r0-r12, lr` (14 words); `lr` is the address of the faulting instruction.
#[unsafe(no_mangle)]
extern "C" fn hrx_exception(kind: u32, regs: *const u32) -> ! {
    let name = match kind {
        1 => "undefined instruction",
        2 => "supervisor call",
        3 => "prefetch abort",
        4 => "data abort",
        _ => "fast interrupt",
    };
    crate::println!("\n*** EXCEPTION: {name}");
    if !regs.is_null() {
        // SAFETY: the entry stub pushed exactly 14 words at `regs`.
        let r = unsafe { core::slice::from_raw_parts(regs, 14) };
        crate::println!("  pc/lr = {:#010x}", r[13]);
        for (i, v) in r[..13].iter().enumerate() {
            crate::println!("  r{i:<2} = {v:#010x}");
        }
    }
    if kind == 4 {
        crate::println!(
            "  DFSR = {:#010x}  DFAR = {:#010x}",
            crate::cp15::dfsr(),
            crate::cp15::dfar()
        );
    }
    if kind == 3 {
        crate::println!(
            "  IFSR = {:#010x}  IFAR = {:#010x}",
            crate::cp15::ifsr(),
            crate::cp15::ifar()
        );
    }
    loop {
        crate::cp15::wfe();
    }
}

/// Declare the program's entry function. It runs on core 0 in System mode after
/// [`start`](self); it must not return.
///
/// ```ignore
/// hardroxide_ps7::entry!(main);
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
