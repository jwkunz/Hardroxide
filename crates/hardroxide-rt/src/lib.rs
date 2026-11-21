//! A FreeRTOS-style real-time kernel for the Zynq-7000's Cortex-A9.
//!
//! [`sched`] is the scheduling policy (pure, host-tested); the rest is the ARM port.

#![cfg_attr(not(test), no_std)]

#[cfg(target_arch = "arm")]
pub mod kernel;
#[cfg(target_arch = "arm")]
pub mod port;
pub mod sched;

#[cfg(target_arch = "arm")]
pub use kernel::*;
