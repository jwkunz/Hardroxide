//! A FreeRTOS-style real-time kernel for one core: the Zynq-7000's Cortex-A9, and the Cortex-A53 of Zynq UltraScale+.
//!
//! [`sched`] is the scheduling policy (pure, host-tested) and `kernel` the API built on it; `port` is the context switch and
//! what else depends on the processor, chosen by the target.

#![cfg_attr(not(test), no_std)]

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
pub mod kernel;
#[cfg(target_arch = "arm")]
#[path = "port/armv7.rs"]
pub mod port;
#[cfg(target_arch = "aarch64")]
#[path = "port/aarch64.rs"]
pub mod port;
pub mod sched;

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
pub use kernel::*;
