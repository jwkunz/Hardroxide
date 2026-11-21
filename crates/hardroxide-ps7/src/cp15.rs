//! Cortex-A9 system control: barriers, interrupt masking, wait-for-event and fault
//! status registers. ARM only. These are the thin `unsafe` wrappers around the
//! instructions; each is a single instruction with no memory-safety effect.

use core::arch::asm;

/// Data synchronisation barrier: all earlier memory accesses complete first.
#[inline(always)]
pub fn dsb() {
    // SAFETY: a barrier instruction.
    unsafe { asm!("dsb sy", options(nostack, preserves_flags)) }
}

/// Data memory barrier.
#[inline(always)]
pub fn dmb() {
    // SAFETY: a barrier instruction.
    unsafe { asm!("dmb sy", options(nostack, preserves_flags)) }
}

/// Instruction synchronisation barrier.
#[inline(always)]
pub fn isb() {
    // SAFETY: a barrier instruction.
    unsafe { asm!("isb sy", options(nostack, preserves_flags)) }
}

/// Wait for an interrupt (low-power idle until one arrives).
#[inline(always)]
pub fn wfi() {
    // SAFETY: waits for an event; no memory effect.
    unsafe { asm!("wfi", options(nostack, preserves_flags)) }
}

/// Wait for an event.
#[inline(always)]
pub fn wfe() {
    // SAFETY: waits for an event; no memory effect.
    unsafe { asm!("wfe", options(nostack, preserves_flags)) }
}

/// Unmask IRQs on this core.
///
/// # Safety
/// Interrupt handlers may run from here on; whatever they use must be initialised.
#[inline(always)]
pub unsafe fn enable_irq() {
    // SAFETY: the caller guarantees handlers are ready.
    unsafe { asm!("cpsie i", options(nostack)) }
}

/// Mask IRQs on this core.
#[inline(always)]
pub fn disable_irq() {
    // SAFETY: masking interrupts cannot violate memory safety.
    unsafe { asm!("cpsid i", options(nostack)) }
}

/// Whether IRQs are currently unmasked.
#[inline(always)]
pub fn irq_enabled() -> bool {
    let cpsr: u32;
    // SAFETY: reads the status register.
    unsafe { asm!("mrs {0}, cpsr", out(reg) cpsr, options(nomem, nostack, preserves_flags)) };
    cpsr & (1 << 7) == 0
}

/// Run `f` with IRQs masked, restoring the previous state afterwards.
#[inline(always)]
pub fn without_irq<T>(f: impl FnOnce() -> T) -> T {
    let was = irq_enabled();
    disable_irq();
    let r = f();
    if was {
        // SAFETY: interrupts were enabled when we were called.
        unsafe { enable_irq() };
    }
    r
}

/// The core number (`MPIDR` affinity 0): 0 or 1 on the Zynq-7000.
#[inline(always)]
pub fn cpu_id() -> u32 {
    let v: u32;
    // SAFETY: reads a read-only register.
    unsafe {
        asm!("mrc p15, 0, {0}, c0, c0, 5", out(reg) v, options(nomem, nostack, preserves_flags))
    };
    v & 3
}

macro_rules! cp15_read {
    ($name:ident, $doc:literal, $crn:literal, $op2:literal) => {
        #[doc = $doc]
        #[inline(always)]
        pub fn $name() -> u32 {
            let v: u32;
            // SAFETY: reads a system register.
            unsafe {
                asm!(concat!("mrc p15, 0, {0}, ", $crn, ", c0, ", $op2),
                     out(reg) v, options(nomem, nostack, preserves_flags))
            };
            v
        }
    };
}

cp15_read!(dfsr, "Data fault status register.", "c5", "0");
cp15_read!(ifsr, "Instruction fault status register.", "c5", "1");
cp15_read!(dfar, "Data fault address register.", "c6", "0");
cp15_read!(ifar, "Instruction fault address register.", "c6", "2");
cp15_read!(sctlr, "System control register.", "c1", "0");

/// The current program status register.
#[inline(always)]
pub fn cpsr() -> u32 {
    let v: u32;
    // SAFETY: reads the status register.
    unsafe { asm!("mrs {0}, cpsr", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}
