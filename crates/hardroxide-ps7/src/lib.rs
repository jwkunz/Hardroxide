//! Zynq-7000 processing-system support for bare metal.
//!
//! The PS must already be initialised (DDR, MIO, PLLs) by the vendor's `ps7_init` or an
//! FSBL; this crate starts a program on core 0 and drives what the program needs from
//! the PS: exception vectors and startup (ARM only), the GIC, the MPCore timers, the
//! Cadence UART and a console. Everything register-level is generic over
//! [`hardroxide_mmio::Mmio`] and unit-tested on the host; the code that only makes sense
//! on a Cortex-A9 is gated on `target_arch = "arm"`.
//!
//! `no_std`, no third-party dependencies.

#![cfg_attr(not(test), no_std)]

pub mod clocks;
#[cfg(target_arch = "arm")]
pub mod console;
#[cfg(target_arch = "arm")]
pub mod cp15;
pub mod devcfg;
pub mod gic;
pub mod irq;
pub mod map;
#[cfg(target_arch = "arm")]
pub mod mem;
#[cfg(target_arch = "arm")]
pub mod time;
pub mod timer;
pub mod uart;
#[cfg(target_arch = "arm")]
pub mod wait;

#[cfg(target_arch = "arm")]
pub mod start;

/// A template for the linker script of a program using this crate: RAM, the section
/// order startup expects, and a stack for each processor mode. The generator fills in the
/// `@...@` placeholders.
pub const LINKER_SCRIPT_TEMPLATE: &str = include_str!("link.x.in");

#[cfg(all(target_arch = "arm", feature = "panic-handler"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("\n*** PANIC: {info}");
    loop {
        cp15::wfe();
    }
}
