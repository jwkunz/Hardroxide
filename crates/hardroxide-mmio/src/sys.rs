//! Real mappings via `mmap`. This is the only place in the workspace that
//! uses `unsafe` for hardware access.

use crate::{check, Mmio, MmioBackend, MmioError};
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

fn io(what: impl Into<String>, source: std::io::Error) -> MmioError {
    MmioError::Io { what: what.into(), source }
}

/// An `mmap`ed device window.
pub struct SysRegion {
    base: *mut u8,   // start of the user-requested window
    map: *mut libc::c_void,
    map_len: usize,
    len: usize,
}

// SAFETY: the mapping is plain device memory addressed through volatile
// accesses; sharing the pointer across threads adds no Rust-level aliasing
// (register access is inherently racy at the hardware level, which is the
// caller's concern, as with any MMIO).
unsafe impl Send for SysRegion {}
unsafe impl Sync for SysRegion {}

impl SysRegion {
    fn mmap(file: &std::fs::File, file_offset: u64, len: usize, what: &str) -> Result<Self, MmioError> {
        // SAFETY: sysconf has no preconditions.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
        let aligned = file_offset & !(page - 1);
        let delta = (file_offset - aligned) as usize;
        let map_len = len + delta;
        // SAFETY: arguments are valid; the result is checked against MAP_FAILED.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                aligned as libc::off_t,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(io(format!("mmap {what}"), std::io::Error::last_os_error()));
        }
        // SAFETY: delta < map_len, so the pointer stays inside the mapping.
        let base = unsafe { (map as *mut u8).add(delta) };
        Ok(SysRegion { base, map, map_len, len })
    }
}

impl Drop for SysRegion {
    fn drop(&mut self) {
        // SAFETY: `map`/`map_len` are exactly what mmap returned.
        unsafe { libc::munmap(self.map, self.map_len) };
    }
}

impl Mmio for SysRegion {
    fn len(&self) -> usize {
        self.len
    }
    fn read32(&self, offset: usize) -> Result<u32, MmioError> {
        check(offset, self.len)?;
        // SAFETY: offset is aligned and in bounds (checked); the mapping lives
        // as long as `self`.
        Ok(unsafe { std::ptr::read_volatile(self.base.add(offset) as *const u32) })
    }
    fn write32(&self, offset: usize, value: u32) -> Result<(), MmioError> {
        check(offset, self.len)?;
        // SAFETY: as in `read32`.
        unsafe { std::ptr::write_volatile(self.base.add(offset) as *mut u32, value) };
        Ok(())
    }
}

/// `/dev/mem` backend. Needs root; may be blocked by `CONFIG_STRICT_DEVMEM`
/// (not for PL address ranges, which are not System RAM).
pub struct DevMem {
    path: PathBuf,
}

impl DevMem {
    pub fn new() -> Self {
        Self { path: "/dev/mem".into() }
    }
    /// Use another file as the "physical memory" (for testing).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Default for DevMem {
    fn default() -> Self {
        Self::new()
    }
}

impl MmioBackend for DevMem {
    type Region = SysRegion;
    fn map(&self, phys: u64, len: usize) -> Result<SysRegion, MmioError> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_SYNC)
            .open(&self.path)
            .map_err(|e| io(format!("open {}", self.path.display()), e))?;
        SysRegion::mmap(&f, phys, len, &self.path.display().to_string())
    }
}

/// UIO backend: maps map 0 of a `/dev/uioN` device. The requested physical
/// address is ignored (the kernel owns the window); `len` must fit in it.
/// Interrupt waiting is not implemented yet (milestone 4).
pub struct Uio {
    path: PathBuf,
}

impl Uio {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl MmioBackend for Uio {
    type Region = SysRegion;
    fn map(&self, _phys: u64, len: usize) -> Result<SysRegion, MmioError> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| io(format!("open {}", self.path.display()), e))?;
        // UIO selects map N with offset N * PAGE_SIZE; map 0 is offset 0.
        SysRegion::mmap(&f, 0, len, &self.path.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_regular_file_at_unaligned_offset() {
        let dir = std::env::temp_dir().join(format!("hrx-mmio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("fake_mem");
        std::fs::write(&p, vec![0u8; 3 * 4096]).unwrap();
        let be = DevMem::with_path(&p);
        let r = be.map(4096 + 0x40, 0x20).unwrap();
        r.write32(8, 0x1234_5678).unwrap();
        assert_eq!(r.read32(8).unwrap(), 0x1234_5678);
        drop(r);
        let bytes = std::fs::read(&p).unwrap();
        let off = 4096 + 0x40 + 8;
        assert_eq!(u32::from_ne_bytes(bytes[off..off + 4].try_into().unwrap()), 0x1234_5678);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
