//! Curated drivers for Xilinx IP, written against [`hardroxide_mmio::Mmio`].

pub mod dma;
pub mod gpio;
pub mod hls;
pub use hardroxide_mmio::{Mmio, MmioError};
