#![cfg_attr(not(test), no_std)]
//! Drivers for chips found on FPGA boards, written against the `embedded-hal` I2C and SPI traits.
//!
//! [`rfsoc4x2`] says how they are wired on the Real Digital RFSoC 4x2. The processor's own blocks (GPIO, SPI, I2C,
//! temperature sensors) are in `hardroxide-psu`.

pub mod at24mac;
pub mod ina220;
pub mod lmk04828;
pub mod lmx2594;
pub mod rfsoc4x2;
pub mod tics;
pub mod us2066;

#[cfg(test)]
mod mock;
