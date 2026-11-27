//! Direct access to identity-mapped physical addresses, for bare metal (no operating
//! system, MMU off or an identity map). There is no `mmap`: a region is just a pointer.

use crate::{Mmio, MmioBackend, MmioError, check};
use core::ptr;

/// A backend that treats physical addresses as pointers.
///
/// Creating one is `unsafe` because every region it hands out is a raw window onto
/// hardware: the caller asserts that physical addresses are identity-mapped and that
/// nothing else uses the windows it maps (the generated `Design` takes this
/// responsibility once, at start-up).
#[derive(Debug, Clone, Copy)]
pub struct PhysMem(());

impl PhysMem {
    /// # Safety
    /// Physical addresses must be directly addressable (identity-mapped, with
    /// device-appropriate memory attributes for register windows), and the windows
    /// later mapped must not be accessed through any other path.
    pub const unsafe fn new() -> Self {
        PhysMem(())
    }
}

impl MmioBackend for PhysMem {
    type Region = PhysRegion;
    fn map(&self, phys: u64, len: usize) -> Result<PhysRegion, MmioError> {
        let base = usize::try_from(phys).map_err(|_| MmioError::BadAddress(phys))?;
        if base.checked_add(len).is_none() {
            return Err(MmioError::BadAddress(phys));
        }
        Ok(PhysRegion {
            base: base as *mut u8,
            len,
        })
    }
}

/// A window of registers at a fixed address.
#[derive(Debug)]
pub struct PhysRegion {
    base: *mut u8,
    len: usize,
}

// SAFETY: a register window is just an address range; moving ownership of it to
// another thread of execution (an RTOS task) is sound. It is deliberately not `Sync`:
// concurrent access to one device from several contexts needs the caller's locking.
unsafe impl Send for PhysRegion {}

impl PhysRegion {
    /// Address of the first register.
    pub fn address(&self) -> usize {
        self.base as usize
    }
}

/// Make earlier normal-memory writes (a DMA descriptor, a buffer) visible before the
/// device access that depends on them. A no-op off ARM.
#[inline(always)]
fn barrier() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    // SAFETY: a data synchronisation barrier has no memory-safety effect.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags))
    };
}

/// On AArch64, keep later memory accesses (to a buffer a device wrote) after the register read that says it is done.
/// The 32-bit ARM build is as it was.
#[inline(always)]
fn after_read() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    #[cfg(target_arch = "aarch64")]
    // SAFETY: a data synchronisation barrier has no memory-safety effect.
    unsafe {
        core::arch::asm!("dsb ld", options(nostack, preserves_flags))
    };
}

impl Mmio for PhysRegion {
    fn len(&self) -> usize {
        self.len
    }
    fn read32(&self, offset: usize) -> Result<u32, MmioError> {
        check(offset, self.len)?;
        // SAFETY: `check` keeps `offset..offset+4` inside the window, 4-byte aligned;
        // the window's validity is the contract of `PhysMem::new`.
        let v = unsafe { ptr::read_volatile(self.base.add(offset) as *const u32) };
        after_read();
        Ok(v)
    }
    fn write32(&self, offset: usize, value: u32) -> Result<(), MmioError> {
        check(offset, self.len)?;
        barrier();
        // SAFETY: as in `read32`.
        unsafe { ptr::write_volatile(self.base.add(offset) as *mut u32, value) };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_a_buffer_standing_in_for_registers() {
        // Host test: a heap buffer plays the register window (the address is a real pointer).
        let mut regs = [0u32; 4];
        let be = unsafe { PhysMem::new() };
        let r = be.map(regs.as_mut_ptr() as u64, 16).unwrap();
        r.write32(4, 0xDEAD_BEEF).unwrap();
        assert_eq!(r.read32(4).unwrap(), 0xDEAD_BEEF);
        r.modify32(4, |v| v & 0xFFFF).unwrap();
        assert_eq!(regs[1], 0xBEEF);
        assert!(matches!(r.read32(16), Err(MmioError::OutOfBounds { .. })));
        assert!(matches!(r.read32(2), Err(MmioError::Unaligned(2))));
    }

    #[test]
    fn rejects_an_address_that_does_not_fit() {
        let be = unsafe { PhysMem::new() };
        if usize::BITS == 32 {
            assert!(matches!(be.map(1 << 40, 4), Err(MmioError::BadAddress(_))));
        }
        assert!(be.map(u64::MAX, 4096).is_err());
    }
}
