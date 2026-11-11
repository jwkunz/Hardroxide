//! Physically contiguous buffers for PL DMA engines.
//!
//! A [`DmaBuffer`] is plain memory the CPU can read and write *and* a
//! physical address a DMA engine can be pointed at. Ownership is tracked by
//! value: handing a buffer to a transfer moves it, so the CPU cannot touch it
//! while hardware may (see `hardroxide_ip::dma::Transfer`).
//!
//! Default backend: [`Udmabuf`] (`u-dma-buf` kernel module), mapped
//! uncached (`O_SYNC`), so no explicit cache maintenance is needed (ADR 0001).

use hardroxide_mmio::{MmioError, SysRegion};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DmaError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot parse `{0}` from {1}")]
    Parse(String, PathBuf),
}

/// Memory usable by both the CPU and a DMA engine.
pub trait DmaBuffer {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Bus/physical address the DMA engine must be given.
    fn phys_addr(&self) -> u64;
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut [u8];
}

/// Ordinary heap memory with a *fake* physical address. For tests with a
/// mock MMIO backend only; hardware cannot DMA to it.
pub struct MockBuffer {
    data: Vec<u8>,
    phys: u64,
}

impl MockBuffer {
    pub fn new(len: usize, phys: u64) -> Self {
        Self {
            data: vec![0; len],
            phys,
        }
    }
}

impl DmaBuffer for MockBuffer {
    fn len(&self) -> usize {
        self.data.len()
    }
    fn phys_addr(&self) -> u64 {
        self.phys
    }
    fn as_slice(&self) -> &[u8] {
        &self.data
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

/// Sysfs/dev locations of a `u-dma-buf` device, configurable for tests.
#[derive(Debug, Clone)]
pub struct UdmabufPaths {
    pub dev: PathBuf,
    pub phys_addr: PathBuf,
    pub size: PathBuf,
}

impl UdmabufPaths {
    pub fn for_index(n: u32) -> Self {
        Self {
            dev: format!("/dev/udmabuf{n}").into(),
            phys_addr: format!("/sys/class/u-dma-buf/udmabuf{n}/phys_addr").into(),
            size: format!("/sys/class/u-dma-buf/udmabuf{n}/size").into(),
        }
    }
}

/// A `u-dma-buf` buffer, mapped uncached. UNVERIFIED on hardware.
pub struct Udmabuf {
    region: SysRegion,
    phys: u64,
}

fn read_num(p: &PathBuf) -> Result<u64, DmaError> {
    let t = std::fs::read_to_string(p).map_err(|source| DmaError::Io {
        path: p.clone(),
        source,
    })?;
    let t = t.trim();
    let r = match t.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => t.parse(),
    };
    r.map_err(|_| DmaError::Parse(t.into(), p.clone()))
}

impl Udmabuf {
    pub fn open(index: u32) -> Result<Self, DmaError> {
        Self::open_with(&UdmabufPaths::for_index(index))
    }

    pub fn open_with(paths: &UdmabufPaths) -> Result<Self, DmaError> {
        let phys = read_num(&paths.phys_addr)?;
        let size = read_num(&paths.size)? as usize;
        // O_SYNC selects an uncached mapping in u-dma-buf.
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc_o_sync())
            .open(&paths.dev)
            .map_err(|source| DmaError::Io {
                path: paths.dev.clone(),
                source,
            })?;
        let region = SysRegion::map_file(&f, 0, size, &paths.dev.display().to_string())?;
        Ok(Self { region, phys })
    }
}

fn libc_o_sync() -> i32 {
    // O_SYNC on Linux (0o4010000); avoids a libc dependency in this crate.
    0o4010000
}

impl DmaBuffer for Udmabuf {
    fn len(&self) -> usize {
        hardroxide_mmio::Mmio::len(&self.region)
    }
    fn phys_addr(&self) -> u64 {
        self.phys
    }
    fn as_slice(&self) -> &[u8] {
        // SAFETY: the mapping is `len()` bytes, valid while `self` lives, and
        // `&self` excludes concurrent `&mut` access from this process. The
        // memory is only written by the DMA engine, which the transfer API
        // keeps from running while a borrow exists (buffer is moved in).
        unsafe { std::slice::from_raw_parts(self.region.as_ptr(), self.len()) }
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: as above, with exclusive access via `&mut self`.
        unsafe { std::slice::from_raw_parts_mut(self.region.as_ptr(), self.len()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udmabuf_maps_fake_device_file() {
        let dir = std::env::temp_dir().join(format!("hrx-udma-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = UdmabufPaths {
            dev: dir.join("dev"),
            phys_addr: dir.join("phys"),
            size: dir.join("size"),
        };
        std::fs::write(&p.dev, vec![0u8; 8192]).unwrap();
        std::fs::write(&p.phys_addr, "0x1f000000\n").unwrap();
        std::fs::write(&p.size, "8192\n").unwrap();
        let mut b = Udmabuf::open_with(&p).unwrap();
        assert_eq!((b.len(), b.phys_addr()), (8192, 0x1f00_0000));
        b.as_mut_slice()[..4].copy_from_slice(&[1, 2, 3, 4]);
        assert_eq!(&b.as_slice()[..4], &[1, 2, 3, 4]);
        drop(b);
        assert_eq!(&std::fs::read(&p.dev).unwrap()[..4], &[1, 2, 3, 4]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
