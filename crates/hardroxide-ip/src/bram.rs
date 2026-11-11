//! AXI BRAM Controller (`xilinx.com:ip:axi_bram_ctrl`, PG078) as a plain
//! word-addressed memory. The window is the block RAM itself, so the driver is
//! a thin, bounds-checked view over [`Mmio`]. UNVERIFIED on hardware.

use hardroxide_mmio::{Mmio, MmioError};

pub struct AxiBram<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiBram<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Size of the memory in bytes.
    pub fn len(&self) -> usize {
        self.regs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regs.is_empty()
    }

    pub fn read_word(&self, offset: usize) -> Result<u32, MmioError> {
        self.regs.read32(offset)
    }

    pub fn write_word(&self, offset: usize, value: u32) -> Result<(), MmioError> {
        self.regs.write32(offset, value)
    }

    /// Fill `out` with consecutive words starting at byte `offset`.
    pub fn read_words(&self, offset: usize, out: &mut [u32]) -> Result<(), MmioError> {
        for (i, w) in out.iter_mut().enumerate() {
            *w = self.regs.read32(offset + 4 * i)?;
        }
        Ok(())
    }

    pub fn write_words(&self, offset: usize, words: &[u32]) -> Result<(), MmioError> {
        for (i, w) in words.iter().enumerate() {
            self.regs.write32(offset + 4 * i, *w)?;
        }
        Ok(())
    }

    /// Read bytes (little-endian words). `offset` must be 4-byte aligned; a
    /// final partial word is read whole and truncated.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), MmioError> {
        for (i, chunk) in out.chunks_mut(4).enumerate() {
            let w = self.regs.read32(offset + 4 * i)?.to_le_bytes();
            chunk.copy_from_slice(&w[..chunk.len()]);
        }
        Ok(())
    }

    /// Write bytes (little-endian words). `offset` must be 4-byte aligned. A
    /// final partial word is read-modify-written so neighbouring bytes keep
    /// their value.
    pub fn write_bytes(&self, offset: usize, data: &[u8]) -> Result<(), MmioError> {
        for (i, chunk) in data.chunks(4).enumerate() {
            let at = offset + 4 * i;
            let mut w = if chunk.len() == 4 {
                [0; 4]
            } else {
                self.regs.read32(at)?.to_le_bytes()
            };
            w[..chunk.len()].copy_from_slice(chunk);
            self.regs.write32(at, u32::from_le_bytes(w))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    fn bram() -> (MockBackend, AxiBram<hardroxide_mmio::MockRegion>) {
        let be = MockBackend::new();
        let r = be.map(0x4000_0000, 0x2000).unwrap();
        (be, AxiBram::new(r))
    }

    #[test]
    fn words_and_bytes_round_trip() {
        let (be, b) = bram();
        assert_eq!(b.len(), 0x2000);
        b.write_words(8, &[1, 2, 3]).unwrap();
        assert_eq!(be.peek(0x4000_0008), 1);
        let mut w = [0u32; 3];
        b.read_words(8, &mut w).unwrap();
        assert_eq!(w, [1, 2, 3]);
        b.write_bytes(0x100, b"hello world").unwrap();
        let mut out = [0u8; 11];
        b.read_bytes(0x100, &mut out).unwrap();
        assert_eq!(&out, b"hello world");
    }

    #[test]
    fn partial_word_write_preserves_neighbours() {
        let (be, b) = bram();
        be.poke(0x4000_0010, 0xAABB_CCDD);
        b.write_bytes(0x10, &[0x11, 0x22]).unwrap();
        assert_eq!(be.peek(0x4000_0010), 0xAABB_2211);
    }

    #[test]
    fn bounds_and_alignment_are_checked() {
        let (_, b) = bram();
        assert!(matches!(
            b.read_word(0x2000),
            Err(MmioError::OutOfBounds { .. })
        ));
        assert!(matches!(
            b.write_bytes(2, &[0]),
            Err(MmioError::Unaligned(2))
        ));
        assert!(b.write_bytes(0x1FFC, &[0; 8]).is_err());
    }
}
