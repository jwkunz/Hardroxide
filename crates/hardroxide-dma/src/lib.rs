//! Physically contiguous buffers for PL DMA engines.
//!
//! A [`DmaBuffer`] is plain memory the CPU can read and write *and* a
//! physical address a DMA engine can be pointed at. Ownership is tracked by
//! value: handing a buffer to a transfer moves it, so the CPU cannot touch it
//! while hardware may (see `hardroxide_ip::dma::Transfer`).
//!
//! Default backend on Linux (`std` feature): [`Udmabuf`] (`u-dma-buf` kernel module),
//! mapped uncached (`O_SYNC`), so no explicit cache maintenance is needed (ADR 0001).
//! On bare metal (no `std`): [`StaticBuffer`], memory the program owns for its whole
//! run, placed by the linker in a region with suitable memory attributes.

#![cfg_attr(not(any(feature = "std", test)), no_std)]

use hardroxide_mmio::MmioError;
#[cfg(feature = "std")]
use hardroxide_mmio::SysRegion;
#[cfg(feature = "std")]
use std::fs::OpenOptions;
#[cfg(feature = "std")]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(feature = "std")]
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DmaError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[cfg(feature = "std")]
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(feature = "std")]
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

/// A buffer in memory that lives for the whole program, for systems without an
/// operating system: a `static` placed by the linker in a region with the right memory
/// attributes (typically non-cacheable, so no cache maintenance is needed).
///
/// The bus address the DMA engine must be given is supplied by the caller: on bare
/// metal with an identity-mapped address space it is the pointer itself
/// ([`identity`](Self::identity)).
pub struct StaticBuffer {
    data: &'static mut [u8],
    phys: u64,
}

impl StaticBuffer {
    /// A buffer whose bus address is `phys`.
    pub fn new(data: &'static mut [u8], phys: u64) -> Self {
        Self { data, phys }
    }
    /// A buffer whose bus address equals its virtual address (identity mapping).
    pub fn identity(data: &'static mut [u8]) -> Self {
        let phys = data.as_ptr() as usize as u64;
        Self { data, phys }
    }
}

/// Define a DMA buffer of `$size` bytes in the `.dma` section (placed by the linker script
/// in its own page-aligned region, so the MMU can give it non-cacheable attributes) and
/// evaluate to `Option<StaticBuffer>`: `Some` the first time the expression runs, `None`
/// afterwards, so the same memory is never handed out twice.
///
/// ```ignore
/// let buf = hardroxide_dma::static_dma_buffer!(4096).expect("taken once");
/// ```
#[macro_export]
macro_rules! static_dma_buffer {
    ($size:expr) => {{
        #[unsafe(link_section = ".dma")]
        static mut BUF: [u8; $size] = [0; $size];
        static TAKEN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
        if TAKEN.swap(true, core::sync::atomic::Ordering::AcqRel) {
            None
        } else {
            // SAFETY: `TAKEN` guarantees this is the only reference ever made to `BUF`.
            Some($crate::StaticBuffer::identity(unsafe {
                &mut *core::ptr::addr_of_mut!(BUF)
            }))
        }
    }};
}

impl DmaBuffer for StaticBuffer {
    fn len(&self) -> usize {
        self.data.len()
    }
    fn phys_addr(&self) -> u64 {
        self.phys
    }
    fn as_slice(&self) -> &[u8] {
        self.data
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.data
    }
}

#[cfg(feature = "std")]
/// Ordinary heap memory with a *fake* physical address. For tests with a
/// mock MMIO backend only; hardware cannot DMA to it.
pub struct MockBuffer {
    data: Vec<u8>,
    phys: u64,
}

#[cfg(feature = "std")]
impl MockBuffer {
    pub fn new(len: usize, phys: u64) -> Self {
        Self {
            data: vec![0; len],
            phys,
        }
    }
}

#[cfg(feature = "std")]
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

#[cfg(feature = "std")]
/// Sysfs/dev locations of a `u-dma-buf` device, configurable for tests.
#[derive(Debug, Clone)]
pub struct UdmabufPaths {
    pub dev: PathBuf,
    pub phys_addr: PathBuf,
    pub size: PathBuf,
}

#[cfg(feature = "std")]
impl UdmabufPaths {
    pub fn for_index(n: u32) -> Self {
        Self {
            dev: format!("/dev/udmabuf{n}").into(),
            phys_addr: format!("/sys/class/u-dma-buf/udmabuf{n}/phys_addr").into(),
            size: format!("/sys/class/u-dma-buf/udmabuf{n}/size").into(),
        }
    }
}

/// A `u-dma-buf` buffer, mapped uncached. Verified on a PYNQ-Z2 (docs/08, G10) and, on AArch64, on an RFSoC 4x2.
#[cfg(feature = "std")]
pub struct Udmabuf {
    region: SysRegion,
    phys: u64,
}

#[cfg(feature = "std")]
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

#[cfg(feature = "std")]
impl Udmabuf {
    pub fn open(index: u32) -> Result<Self, DmaError> {
        Self::open_with(&UdmabufPaths::for_index(index))
    }

    /// On AArch64 this first puts a device that is in `u-dma-buf`'s default synchronisation mode (1) into its
    /// write-combining mode (2), through the `sync_mode` file beside `size`; the setting is the device's, not this
    /// handle's.
    pub fn open_with(paths: &UdmabufPaths) -> Result<Self, DmaError> {
        let phys = read_num(&paths.phys_addr)?;
        let size = read_num(&paths.size)? as usize;
        if cfg!(target_arch = "aarch64") {
            use_normal_memory(&paths.size.with_file_name("sync_mode"))?;
        }
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

/// In its default mode `u-dma-buf` maps an `O_SYNC` buffer on AArch64 as *device* memory, where the unaligned
/// accesses that ordinary slice operations make fault (a bus error). Its mode 2 is uncached too, but normal memory.
/// Other modes are someone's choice and stay; so does the "always" bit. Nothing to do if there is no such file.
#[cfg(feature = "std")]
fn use_normal_memory(sync_mode: &std::path::Path) -> Result<(), DmaError> {
    const MODE: u64 = 3;
    const NON_CACHED: u64 = 1;
    const WRITE_COMBINE: u64 = 2;
    if !sync_mode.exists() {
        return Ok(());
    }
    let path = sync_mode.to_path_buf();
    let now = read_num(&path)?;
    if now & MODE == NON_CACHED {
        std::fs::write(&path, format!("{}", now & !MODE | WRITE_COMBINE))
            .map_err(|source| DmaError::Io { path, source })?;
    }
    Ok(())
}

#[cfg(feature = "std")]
fn libc_o_sync() -> i32 {
    // O_SYNC on Linux (0o4010000); avoids a libc dependency in this crate.
    0o4010000
}

#[cfg(feature = "std")]
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

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn only_the_default_sync_mode_is_changed() {
        let file = std::env::temp_dir().join(format!("hrx-sync-mode-{}", std::process::id()));
        let _ = std::fs::remove_file(&file);
        use_normal_memory(&file).unwrap();
        assert!(!file.exists());
        for (before, after) in [("1", "2"), ("5", "6"), ("0", "0"), ("2", "2"), ("3", "3")] {
            std::fs::write(&file, format!("{before}\n")).unwrap();
            use_normal_memory(&file).unwrap();
            assert_eq!(
                std::fs::read_to_string(&file).unwrap().trim(),
                after,
                "{before}"
            );
        }
        std::fs::remove_file(file).unwrap();
    }

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

#[cfg(test)]
mod static_buffer_tests {
    use super::*;

    #[test]
    fn the_static_buffer_macro_hands_out_each_buffer_once() {
        // One expansion is one static: it can be taken once, however often it is evaluated.
        let take = || static_dma_buffer!(64);
        let first = take().expect("first call gets the buffer");
        assert_eq!(first.len(), 64);
        assert!(take().is_none());
        assert!(take().is_none());
        // A different expansion is a different buffer.
        let other = || static_dma_buffer!(8);
        assert_eq!(other().unwrap().len(), 8);
        assert!(other().is_none());
    }

    #[test]
    fn a_static_buffer_reports_its_address_and_gives_access() {
        let mem: &'static mut [u8] = std::boxed::Box::leak(std::boxed::Box::new([0u8; 32]));
        let addr = mem.as_ptr() as usize as u64;
        let mut b = StaticBuffer::identity(mem);
        assert_eq!((b.len(), b.phys_addr()), (32, addr));
        b.as_mut_slice()[3] = 7;
        assert_eq!(b.as_slice()[3], 7);
        let other: &'static mut [u8] = std::boxed::Box::leak(std::boxed::Box::new([0u8; 4]));
        assert_eq!(
            StaticBuffer::new(other, 0x1F00_0000).phys_addr(),
            0x1F00_0000
        );
    }
}
