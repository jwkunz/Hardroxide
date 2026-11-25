//! Cortex-A53 system control at EL3: barriers, interrupt masking, wait-for-event and the
//! exception registers. AArch64 only. These are thin wrappers around single instructions.
//!
//! The functions both processor families have carry the names `hardroxide_ps7::cpu` has.

use core::arch::asm;

macro_rules! instruction {
    ($(#[$doc:meta])* $name:ident, $text:literal) => {
        $(#[$doc])*
        #[inline(always)]
        pub fn $name() {
            // SAFETY: a barrier or hint instruction with no memory-safety effect.
            unsafe { asm!($text, options(nostack, preserves_flags)) }
        }
    };
}

instruction!(
    /// Data synchronisation barrier: all earlier memory accesses complete first.
    dsb,
    "dsb sy"
);
instruction!(
    /// Data memory barrier.
    dmb,
    "dmb sy"
);
instruction!(
    /// Instruction synchronisation barrier.
    isb,
    "isb"
);
instruction!(
    /// Wait for an interrupt (low-power idle until one arrives).
    wfi,
    "wfi"
);
instruction!(
    /// Wait for an event.
    wfe,
    "wfe"
);

/// Unmask IRQs on this core.
///
/// # Safety
/// Interrupt handlers may run from here on; whatever they use must be initialised.
#[inline(always)]
pub unsafe fn enable_irq() {
    // SAFETY: the caller guarantees handlers are ready.
    unsafe { asm!("msr daifclr, #2", options(nostack)) }
}

/// Mask IRQs on this core.
#[inline(always)]
pub fn disable_irq() {
    // SAFETY: masking interrupts cannot violate memory safety.
    unsafe { asm!("msr daifset, #2", options(nostack)) }
}

/// Whether IRQs are currently unmasked.
#[inline(always)]
pub fn irq_enabled() -> bool {
    daif() & (1 << 7) == 0
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

macro_rules! system_register {
    ($(#[$doc:meta])* $name:ident, $register:literal) => {
        $(#[$doc])*
        #[inline(always)]
        pub fn $name() -> u64 {
            let v: u64;
            // SAFETY: reads a system register.
            unsafe {
                asm!(concat!("mrs {0}, ", $register), out(reg) v, options(nomem, nostack, preserves_flags))
            };
            v
        }
    };
}

system_register!(
    /// The interrupt mask bits.
    daif,
    "daif"
);
system_register!(
    /// System control register of EL3.
    sctlr,
    "sctlr_el3"
);
system_register!(
    /// Syndrome of the last exception taken to EL3.
    esr,
    "esr_el3"
);
system_register!(
    /// Address of the last memory fault taken to EL3.
    far,
    "far_el3"
);
system_register!(
    /// Return address of the last exception taken to EL3.
    elr,
    "elr_el3"
);
system_register!(
    /// The affinity register.
    mpidr,
    "mpidr_el1"
);
system_register!(
    /// The current exception level, in bits 3:2.
    current_el_register,
    "CurrentEL"
);

/// The core number (`MPIDR` affinity 0): 0 to 3 on the Zynq UltraScale+ APU.
#[inline(always)]
pub fn cpu_id() -> u32 {
    (mpidr() & 0xFF) as u32
}

/// The exception level the caller runs at (3 for a program started by this crate).
#[inline(always)]
pub fn current_el() -> u32 {
    ((current_el_register() >> 2) & 3) as u32
}
