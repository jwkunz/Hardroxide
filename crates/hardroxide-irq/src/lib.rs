//! Interrupt waiting: the platform-neutral pieces ([`IrqLine`], [`IrqWaiter`],
//! [`IrqError`]) and, with the `std` feature, a Linux UIO device (`/dev/uioN`) backend.
//!
//! UIO protocol: writing a 32-bit `1` unmasks the interrupt; a blocking
//! 4-byte read returns the cumulative event count once it fires (the kernel
//! masks the line while it is pending). Verified on a PYNQ-Z2 (docs/08, G11); note that a GIC line keeps the
//! trigger type of the first overlay that mapped it until reboot.
//!
//! Without `std` the crate is `#![no_std]`.

#![cfg_attr(not(any(feature = "std", test)), no_std)]

use core::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum IrqError {
    #[cfg(feature = "std")]
    #[error("I/O error on {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "no UIO device for interrupt `{instance}.{port}` (device-tree node `{node}`, base {base:#x}); \
         is the generated overlay (interrupts.dtso) applied?"
    )]
    NotFound {
        instance: &'static str,
        port: &'static str,
        node: &'static str,
        base: u64,
    },
    #[error(
        "interrupt `{instance}.{port}` feeds input {bit} of the interrupt controller \
         `{controller}`; open that controller's own `IRQ_*` line and use the `AxiIntc` driver"
    )]
    ViaController {
        instance: &'static str,
        port: &'static str,
        controller: &'static str,
        bit: u32,
    },
    /// A platform's interrupt layer (the bare-metal GIC, an RTOS) could not do what
    /// was asked.
    #[error("interrupt handling failed: {0}")]
    Platform(&'static str),
}

/// One interrupt output of a PL IP and where it lands. The generator emits a
/// constant per connected interrupt in the generated crate's `pac`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqLine {
    pub instance: &'static str,
    pub port: &'static str,
    /// Bit of the PS7 `IRQ_F2P` vector, if it could be traced.
    pub f2p_bit: Option<u32>,
    /// Zynq-7000 GIC SPI device-tree cell for that bit.
    pub gic_spi: Option<u32>,
    /// Name of the `uio` node in the generated overlay (`interrupts.dtso`).
    pub dt_node: &'static str,
    /// Physical base address of the IP's register window.
    pub base: u64,
    /// `(controller instance, input bit)` if the signal feeds an AXI
    /// interrupt controller rather than the PS directly; then there is no UIO
    /// device of its own (use the controller's line).
    pub via_intc: Option<(&'static str, u32)>,
}

/// Something that can be unmasked and waited on; implemented by `UioIrq` on Linux (`std` feature) and by the GIC
/// layer on bare metal.
/// Drivers that multiplex interrupts (the AXI interrupt controller) are
/// generic over it so they can be tested without a device.
pub trait IrqWaiter {
    /// Unmask the interrupt.
    fn enable(&self) -> Result<(), IrqError>;
    /// Wait for the next event without re-enabling; `None` on timeout.
    fn wait_event(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError>;
}

#[cfg(feature = "std")]
mod uio;
#[cfg(feature = "std")]
pub use uio::*;
