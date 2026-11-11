//! Curated drivers for Xilinx IP, written against [`hardroxide_mmio::Mmio`].

pub mod gpio;
pub use hardroxide_mmio::{Mmio, MmioError};
