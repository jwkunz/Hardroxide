//! Block devices: storage addressed in 512-byte blocks (an SD card, a disk image in memory).
//!
//! The SD host driver in `hardroxide-ps7` and the FAT file system in `hardroxide-fat` meet here, so
//! neither depends on the other.

/// The size of a block in bytes.
pub const BLOCK_SIZE: usize = 512;

/// One block.
pub type Block = [u8; BLOCK_SIZE];

/// Storage addressed in [`BLOCK_SIZE`]-byte blocks.
pub trait BlockDevice {
    type Error: core::fmt::Debug;

    /// Number of blocks on the device.
    fn block_count(&self) -> u64;

    /// Read the blocks starting at `lba` into `buf`, whose length is a multiple of [`BLOCK_SIZE`].
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), Self::Error>;

    /// Write `buf` (a multiple of [`BLOCK_SIZE`] long) to the blocks starting at `lba`.
    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), Self::Error>;

    /// Make sure everything written has reached the medium. The default does nothing.
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// An error from [`SliceDisk`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SliceDiskError {
    #[error("blocks {lba}.. are past the end of the {blocks}-block disk")]
    OutOfRange { lba: u64, blocks: u64 },
    #[error("a transfer must be a whole number of 512-byte blocks")]
    NotBlocks,
}

/// A disk in memory: a byte slice treated as blocks. For tests and for images loaded into RAM.
pub struct SliceDisk<'a> {
    bytes: &'a mut [u8],
}

impl<'a> SliceDisk<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        SliceDisk { bytes }
    }

    fn range(&self, lba: u64, len: usize) -> Result<core::ops::Range<usize>, SliceDiskError> {
        if len % BLOCK_SIZE != 0 {
            return Err(SliceDiskError::NotBlocks);
        }
        let blocks = self.block_count();
        let start = lba.checked_mul(BLOCK_SIZE as u64).filter(|_| lba < blocks || len == 0);
        match start {
            Some(s) if s as usize + len <= self.bytes.len() => Ok(s as usize..s as usize + len),
            _ => Err(SliceDiskError::OutOfRange { lba, blocks }),
        }
    }
}

impl BlockDevice for SliceDisk<'_> {
    type Error = SliceDiskError;

    fn block_count(&self) -> u64 {
        (self.bytes.len() / BLOCK_SIZE) as u64
    }

    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), SliceDiskError> {
        let r = self.range(lba, buf.len())?;
        buf.copy_from_slice(&self.bytes[r]);
        Ok(())
    }

    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), SliceDiskError> {
        let r = self.range(lba, buf.len())?;
        self.bytes[r].copy_from_slice(buf);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slice_disk_reads_and_writes_blocks() {
        let mut mem = [0u8; 4 * BLOCK_SIZE];
        let mut d = SliceDisk::new(&mut mem);
        assert_eq!(d.block_count(), 4);
        d.write_blocks(1, &[7u8; 2 * BLOCK_SIZE]).unwrap();
        let mut b = [0u8; BLOCK_SIZE];
        d.read_blocks(2, &mut b).unwrap();
        assert!(b.iter().all(|&x| x == 7));
        d.read_blocks(0, &mut b).unwrap();
        assert!(b.iter().all(|&x| x == 0));
    }

    #[test]
    fn out_of_range_and_partial_blocks_are_refused() {
        let mut mem = [0u8; 2 * BLOCK_SIZE];
        let mut d = SliceDisk::new(&mut mem);
        let mut b = [0u8; BLOCK_SIZE];
        assert!(matches!(d.read_blocks(2, &mut b), Err(SliceDiskError::OutOfRange { .. })));
        assert!(matches!(d.read_blocks(0, &mut [0u8; 100]), Err(SliceDiskError::NotBlocks)));
        assert!(matches!(d.write_blocks(1, &[0u8; 2 * BLOCK_SIZE]), Err(SliceDiskError::OutOfRange { .. })));
    }
}
