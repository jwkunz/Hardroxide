use crate::{Mmio, MmioBackend, MmioError, check};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// One recorded register access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read { addr: u64, value: u32 },
    Write { addr: u64, value: u32 },
}

#[derive(Default)]
struct State {
    mem: BTreeMap<u64, u32>,
    w1c: BTreeMap<u64, u32>,
    log: Vec<Access>,
}

/// Sparse, shared fake physical memory. Registers read back what was
/// written (0 if never written); tests can [`poke`](Self::poke) values the
/// hardware would produce and inspect the access log.
#[derive(Clone, Default)]
pub struct MockBackend(Arc<Mutex<State>>);

impl MockBackend {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn poke(&self, phys: u64, value: u32) {
        self.0.lock().unwrap().mem.insert(phys, value);
    }
    pub fn peek(&self, phys: u64) -> u32 {
        self.0.lock().unwrap().mem.get(&phys).copied().unwrap_or(0)
    }
    /// Mark `phys` as a write-1-to-clear register: a write clears the bits
    /// that are set in the written value instead of storing it.
    pub fn set_write1_clear(&self, phys: u64) {
        self.set_write1_clear_mask(phys, u32::MAX);
    }
    /// Like [`set_write1_clear`](Self::set_write1_clear) but only for the
    /// bits in `mask`; other bits are stored normally.
    pub fn set_write1_clear_mask(&self, phys: u64, mask: u32) {
        self.0.lock().unwrap().w1c.insert(phys, mask);
    }
    /// Accesses made through regions, in order.
    pub fn log(&self) -> Vec<Access> {
        self.0.lock().unwrap().log.clone()
    }
}

pub struct MockRegion {
    base: u64,
    len: usize,
    state: Arc<Mutex<State>>,
}

impl MmioBackend for MockBackend {
    type Region = MockRegion;
    fn map(&self, phys: u64, len: usize) -> Result<MockRegion, MmioError> {
        Ok(MockRegion {
            base: phys,
            len,
            state: self.0.clone(),
        })
    }
}

impl Mmio for MockRegion {
    fn len(&self) -> usize {
        self.len
    }
    fn read32(&self, offset: usize) -> Result<u32, MmioError> {
        check(offset, self.len)?;
        let addr = self.base + offset as u64;
        let mut s = self.state.lock().unwrap();
        let value = s.mem.get(&addr).copied().unwrap_or(0);
        s.log.push(Access::Read { addr, value });
        Ok(value)
    }
    fn write32(&self, offset: usize, value: u32) -> Result<(), MmioError> {
        check(offset, self.len)?;
        let addr = self.base + offset as u64;
        let mut s = self.state.lock().unwrap();
        if let Some(&mask) = s.w1c.get(&addr) {
            let old = s.mem.get(&addr).copied().unwrap_or(0);
            s.mem.insert(addr, (value & !mask) | (old & mask & !value));
        } else {
            s.mem.insert(addr, value);
        }
        s.log.push(Access::Write { addr, value });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bounds_and_log() {
        let b = MockBackend::new();
        let r = b.map(0x4120_0000, 0x10).unwrap();
        r.write32(4, 0xdead_beef).unwrap();
        assert_eq!(r.read32(4).unwrap(), 0xdead_beef);
        assert_eq!(b.peek(0x4120_0004), 0xdead_beef);
        assert!(matches!(r.read32(0x10), Err(MmioError::OutOfBounds { .. })));
        assert!(matches!(r.read32(2), Err(MmioError::Unaligned(2))));
        assert_eq!(b.log().len(), 2);
        b.set_write1_clear(0x4120_0008);
        b.poke(0x4120_0008, 0b1011);
        r.write32(8, 0b0011).unwrap();
        assert_eq!(b.peek(0x4120_0008), 0b1000);
    }
}
