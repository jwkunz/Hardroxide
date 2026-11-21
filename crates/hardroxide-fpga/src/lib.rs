//! Bitstream loading (Linux `fpga_manager` sysfs, `std` feature) and PL clock control
//! (Zynq-7000 SLCR registers over an [`Mmio`](hardroxide_mmio::Mmio) window, always).
//!
//! Without the `std` feature the crate is `#![no_std]` and offers only the SLCR clock
//! code, which bare-metal programs use. Sysfs paths are configurable so the loader can
//! be tested on a fake tree; it is verified on a PYNQ-Z2 (docs/08, G1, G2).

#![cfg_attr(not(any(feature = "std", test)), no_std)]

pub mod slcr;

#[cfg(feature = "std")]
mod host;
#[cfg(feature = "std")]
pub use host::*;

#[derive(Debug, thiserror::Error)]
pub enum FpgaError {
    #[cfg(feature = "std")]
    #[error("bitstream hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[cfg(feature = "std")]
    #[error("I/O error on {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(feature = "std")]
    #[error("fpga_manager did not reach `operating` state after load (state: `{0}`)")]
    NotOperating(String),
    #[cfg(feature = "std")]
    #[error(
        "kernel drivers are bound to PL addresses and would be running while the PL is reprogrammed, \
         which can freeze the CPU: {0}. Stop and unbind them first (see the \"Running on hardware\" chapter), \
         or set `FpgaPaths::refuse_bound_pl_drivers` to false"
    )]
    PlInUse(String),
    #[error("PL clock FCLK{0} does not exist (valid: 0..=3)")]
    BadClockIndex(u8),
    #[error(
        "cannot reach {requested} Hz on FCLK{index}: best is {best} Hz from a {source_hz} Hz source"
    )]
    ClockUnreachable {
        index: u8,
        requested: u64,
        best: u64,
        source_hz: u64,
    },
    #[error("PLL feeding FCLK{0} is bypassed or has a zero feedback divider")]
    PllNotRunning(u8),
    #[error(transparent)]
    Mmio(#[from] hardroxide_mmio::MmioError),
}
