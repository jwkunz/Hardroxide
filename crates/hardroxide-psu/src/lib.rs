//! Zynq UltraScale+ processing-system support for bare metal on the Cortex-A53.
//!
//! The PS must already be initialised (DDR, MIO, PLLs) by the vendor's `psu_init` or an
//! FSBL; this crate starts a program on core 0 **in AArch64 at EL3**, the state the
//! processor is in after reset over JTAG, and drives what the program needs from the PS:
//! exception vectors and startup, the MMU and caches, the GIC, the generic timer, the
//! clocks, the Cadence UART and a console. Everything register-level is generic over
//! [`hardroxide_mmio::Mmio`] and unit-tested on the host; the code that only makes sense
//! on a Cortex-A53 is gated on `target_arch = "aarch64"`.
//!
//! This is the counterpart of [`hardroxide-ps7`](hardroxide_ps7) for the Zynq-7000, with the
//! same module names where the hardware allows it. The drivers for peripheral blocks both
//! families have ([`uart`], [`i2c`], [`spi`], [`sd`], and the Ethernet MAC in [`gem`]) are that crate's, re-exported.
//!
//! `no_std`, no third-party dependencies.

#![cfg_attr(not(test), no_std)]

pub use hardroxide_ps7::{i2c, sd, spi, uart};
pub mod gem;
pub mod gpio;
pub mod pl;
pub mod sysmon;

pub mod clocks;
#[cfg(target_arch = "aarch64")]
pub mod console;
#[cfg(target_arch = "aarch64")]
pub mod cpu;
pub mod gic;
pub mod irq;
pub mod map;
pub mod mem;
#[cfg(target_arch = "aarch64")]
pub mod time;
pub mod timer;
#[cfg(target_arch = "aarch64")]
pub mod wait;

#[cfg(target_arch = "aarch64")]
pub mod start;

/// A template for the linker script of a program using this crate: RAM, the section
/// order startup expects, and the two stacks. The generator fills in the `@...@`
/// placeholders.
pub const LINKER_SCRIPT_TEMPLATE: &str = include_str!("link.x.in");

#[cfg(all(target_arch = "aarch64", feature = "panic-handler"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("\n*** PANIC: {info}");
    loop {
        cpu::wfe();
    }
}
