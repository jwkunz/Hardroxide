//! Pluggable memory-mapped I/O backends.
//!
//! Drivers are written against [`Mmio`]; a [`MmioBackend`] turns a physical
//! address window into a region. Backends: [`DevMem`] (`/dev/mem`),
//! [`Uio`] (`/dev/uioN`) and [`MockBackend`] (hardware-free tests).

mod mock;
mod sys;

pub use mock::{Access as MockAccess, MockBackend, MockRegion};
pub use sys::{DevMem, SysRegion, Uio};

#[derive(Debug, thiserror::Error)]
pub enum MmioError {
    #[error("offset {offset:#x} (+4) is outside the {len:#x}-byte region")]
    OutOfBounds { offset: usize, len: usize },
    #[error("offset {0:#x} is not 4-byte aligned")]
    Unaligned(usize),
    #[error("I/O error on {what}: {source}")]
    Io {
        what: String,
        #[source]
        source: std::io::Error,
    },
}

/// A mapped window of device registers. Offsets are in bytes from the start
/// of the window. Methods take `&self`: a register write is not a Rust-level
/// mutation of memory the program owns.
pub trait Mmio {
    /// Window length in bytes.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read32(&self, offset: usize) -> Result<u32, MmioError>;
    fn write32(&self, offset: usize, value: u32) -> Result<(), MmioError>;

    /// Read-modify-write helper.
    fn modify32(&self, offset: usize, f: impl FnOnce(u32) -> u32) -> Result<(), MmioError> {
        let v = self.read32(offset)?;
        self.write32(offset, f(v))
    }
}

pub trait MmioBackend {
    type Region: Mmio;
    /// Map `len` bytes of physical address space starting at `phys`.
    fn map(&self, phys: u64, len: usize) -> Result<Self::Region, MmioError>;
}

pub(crate) fn check(offset: usize, len: usize) -> Result<(), MmioError> {
    if !offset.is_multiple_of(4) {
        return Err(MmioError::Unaligned(offset));
    }
    if offset.checked_add(4).is_none_or(|e| e > len) {
        return Err(MmioError::OutOfBounds { offset, len });
    }
    Ok(())
}
