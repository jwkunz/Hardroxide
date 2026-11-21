//! AXI Video Direct Memory Access (`xilinx.com:ip:axi_vdma`, PG020), register direct mode, read
//! channel (MM2S: frame buffers in memory out to a video stream, the display side). The register
//! layout follows the vendor driver's header.
//!
//! The read channel runs in **park mode**: it repeats one chosen frame buffer, and
//! [`AxiVdma::show_frame`] switches to another at the next frame boundary, which is all that
//! double buffering needs. The write channel (stream to memory, for video capture) is not wrapped yet.

use core::time::Duration;
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{Mmio, MmioError};

const MM2S_CR: usize = 0x00;
const MM2S_SR: usize = 0x04;
const PARK_PTR: usize = 0x28;
const VERSION: usize = 0x2C;
const MM2S_VSIZE: usize = 0x50;
const MM2S_HSIZE: usize = 0x54;
const MM2S_STRIDE: usize = 0x58;
const MM2S_START_ADDR: usize = 0x5C;

const CR_RUN: u32 = 1 << 0;
const CR_RESET: u32 = 1 << 2;
const CR_FRMCNT_SHIFT: u32 = 16;
const CR_IRQ_FRMCNT: u32 = 1 << 12;
const CR_IRQ_ERROR: u32 = 1 << 14;
const SR_HALTED: u32 = 1 << 0;
const SR_ERRORS: u32 = 0x0000_0FF0;
const SR_IRQ_FRMCNT: u32 = 1 << 12;
const SR_IRQ_ALL: u32 = 0x0000_7000;
const PARK_READ_MASK: u32 = 0x1F;
const PARK_READ_CURRENT_SHIFT: u32 = 16;

/// Most frame buffers the core can hold addresses for in this driver.
pub const MAX_FRAMES: usize = 16;
/// Start-address slots always written (the base overlay's core has four).
const MIN_SLOTS: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum VdmaError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("the channel did not reset")]
    ResetTimeout,
    #[error("the channel reports errors (status {0:#x})")]
    Errors(u32),
    #[error("a frame buffer address, size or count is out of range")]
    BadFrame,
}

/// Frame geometry for a read: `hsize` bytes per line, `stride` bytes between line starts,
/// `lines` lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub hsize: u32,
    pub stride: u32,
    pub lines: u32,
}

pub struct AxiVdma<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiVdma<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// The core's version register.
    pub fn version(&self) -> Result<u32, MmioError> {
        self.regs.read32(VERSION)
    }

    /// Reset the read channel and wait for the reset bit to clear.
    pub fn reset_read(&self) -> Result<(), VdmaError> {
        self.regs.write32(MM2S_CR, CR_RESET)?;
        let t0 = Instant::now();
        while self.regs.read32(MM2S_CR)? & CR_RESET != 0 {
            if t0.elapsed() > Duration::from_millis(100) {
                return Err(VdmaError::ResetTimeout);
            }
            sleep(Duration::from_micros(10));
        }
        Ok(())
    }

    /// Start the read channel on `frames` (32-bit physical addresses), parked on frame 0, with
    /// the frame geometry `g`. The core then streams the parked frame forever, one frame per
    /// vertical sync of the consumer.
    pub fn start_read(&self, frames: &[u32], g: &Geometry) -> Result<(), VdmaError> {
        if frames.is_empty()
            || frames.len() > MAX_FRAMES
            || g.lines == 0
            || g.lines > 0x1FFF
            || g.hsize > 0xFFFF
            || g.stride > 0xFFFF
        {
            return Err(VdmaError::BadFrame);
        }
        self.reset_read()?;
        self.configure_read(frames, g)
    }

    /// The register sequence of [`AxiVdma::start_read`] after the reset (separate so the mock backend,
    /// which cannot clear the self-clearing reset bit, can test it).
    fn configure_read(&self, frames: &[u32], g: &Geometry) -> Result<(), VdmaError> {
        let r = &self.regs;
        // Park mode (circular bit clear). The core's start-address slots are all written, the given
        // frames repeating, as the Linux driver does; the frame-store register is left alone.
        r.write32(MM2S_CR, 1 << CR_FRMCNT_SHIFT)?;
        r.write32(PARK_PTR, 0)?;
        for i in 0..frames.len().max(MIN_SLOTS) {
            r.write32(MM2S_START_ADDR + 4 * i, frames[i % frames.len()])?;
        }
        r.write32(MM2S_CR, 1 << CR_FRMCNT_SHIFT | CR_RUN)?;
        r.write32(MM2S_STRIDE, g.stride)?;
        r.write32(MM2S_HSIZE, g.hsize)?;
        // Writing the vertical size starts the transfer.
        r.write32(MM2S_VSIZE, g.lines)?;
        Ok(())
    }

    /// Stop the read channel (it finishes the frame in progress).
    pub fn stop_read(&self) -> Result<(), MmioError> {
        let cr = self.regs.read32(MM2S_CR)?;
        self.regs.write32(MM2S_CR, cr & !CR_RUN)
    }

    /// Whether the read channel is halted.
    pub fn read_halted(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(MM2S_SR)? & SR_HALTED != 0)
    }

    /// Error bits of the read status; `Ok(())` if none.
    pub fn check_read(&self) -> Result<(), VdmaError> {
        let sr = self.regs.read32(MM2S_SR)?;
        if sr & SR_ERRORS != 0 {
            Err(VdmaError::Errors(sr))
        } else {
            Ok(())
        }
    }

    /// Switch the display to frame buffer `n` at the next frame boundary.
    pub fn show_frame(&self, n: u32) -> Result<(), MmioError> {
        let p = self.regs.read32(PARK_PTR)?;
        self.regs
            .write32(PARK_PTR, (p & !PARK_READ_MASK) | (n & PARK_READ_MASK))
    }

    /// The frame buffer the read channel is streaming right now.
    pub fn current_frame(&self) -> Result<u32, MmioError> {
        Ok((self.regs.read32(PARK_PTR)? >> PARK_READ_CURRENT_SHIFT) & PARK_READ_MASK)
    }

    /// Raise an interrupt at the end of every frame (and on errors).
    pub fn enable_frame_interrupt(&self) -> Result<(), MmioError> {
        let cr = self.regs.read32(MM2S_CR)?;
        self.regs
            .write32(MM2S_CR, cr | CR_IRQ_FRMCNT | CR_IRQ_ERROR)
    }

    pub fn disable_interrupts(&self) -> Result<(), MmioError> {
        let cr = self.regs.read32(MM2S_CR)?;
        self.regs
            .write32(MM2S_CR, cr & !(CR_IRQ_FRMCNT | CR_IRQ_ERROR))
    }

    /// Whether a frame has completed since the last [`AxiVdma::ack_frame`].
    pub fn frame_done(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(MM2S_SR)? & SR_IRQ_FRMCNT != 0)
    }

    /// Clear the frame-done flag (and any other interrupt flag); returns what was set.
    pub fn ack_frame(&self) -> Result<u32, MmioError> {
        let sr = self.regs.read32(MM2S_SR)?;
        self.regs.write32(MM2S_SR, sr & SR_IRQ_ALL)?;
        Ok(sr & SR_IRQ_ALL)
    }

    /// Wait for the next frame boundary (polling the frame-done flag), up to `timeout`.
    pub fn wait_frame(&self, timeout: Duration) -> Result<bool, MmioError> {
        self.ack_frame()?;
        let t0 = Instant::now();
        while !self.frame_done()? {
            if t0.elapsed() > timeout {
                return Ok(false);
            }
            core::hint::spin_loop();
        }
        self.ack_frame()?;
        Ok(true)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const BASE: u64 = 0x4300_0000;

    #[test]
    fn start_read_parks_on_frame_zero_and_writes_the_size_last() {
        let be = MockBackend::new();
        let v = AxiVdma::new(be.map(BASE, 0x1000).unwrap());
        let g = Geometry {
            hsize: 5120,
            stride: 5120,
            lines: 720,
        };
        v.configure_read(&[0x1000_0000, 0x1040_0000], &g).unwrap();
        let p = |off: usize| be.peek(BASE + off as u64);
        assert_eq!(p(MM2S_START_ADDR), 0x1000_0000);
        assert_eq!(p(MM2S_START_ADDR + 4), 0x1040_0000);
        // Four slots, the two frames repeating.
        assert_eq!(p(MM2S_START_ADDR + 8), 0x1000_0000);
        assert_eq!(p(MM2S_START_ADDR + 12), 0x1040_0000);
        assert_eq!(p(MM2S_HSIZE), 5120);
        assert_eq!(p(MM2S_STRIDE), 5120);
        assert_eq!(p(MM2S_VSIZE), 720);
        assert_eq!(p(MM2S_CR) & CR_RUN, CR_RUN);
        assert_eq!(p(PARK_PTR) & 0x1F, 0);
        // The vertical size is the last write: it starts the transfer.
        let writes: alloc::vec::Vec<u64> = be
            .log()
            .iter()
            .filter_map(|a| match a {
                hardroxide_mmio::MockAccess::Write { addr, .. } => Some(*addr - BASE),
                _ => None,
            })
            .collect();
        assert_eq!(*writes.last().unwrap(), MM2S_VSIZE as u64);
    }

    #[test]
    fn show_frame_changes_only_the_read_reference() {
        let be = MockBackend::new();
        let v = AxiVdma::new(be.map(BASE, 0x1000).unwrap());
        be.poke(BASE + PARK_PTR as u64, 0x0300_0000); // a write reference the driver must keep
        v.show_frame(1).unwrap();
        assert_eq!(be.peek(BASE + PARK_PTR as u64), 0x0300_0001);
        be.poke(BASE + PARK_PTR as u64, 0x0001_0001);
        assert_eq!(v.current_frame().unwrap(), 1);
    }

    #[test]
    fn bad_geometry_is_refused() {
        let be = MockBackend::new();
        let v = AxiVdma::new(be.map(BASE, 0x1000).unwrap());
        let g = Geometry {
            hsize: 1,
            stride: 1,
            lines: 0,
        };
        assert!(matches!(v.start_read(&[0], &g), Err(VdmaError::BadFrame)));
        let g = Geometry {
            hsize: 1,
            stride: 1,
            lines: 1,
        };
        assert!(matches!(v.start_read(&[], &g), Err(VdmaError::BadFrame)));
    }

    #[test]
    fn ack_clears_the_flags_that_were_set() {
        let be = MockBackend::new();
        let v = AxiVdma::new(be.map(BASE, 0x1000).unwrap());
        be.poke(BASE + MM2S_SR as u64, SR_IRQ_FRMCNT | 1);
        assert!(v.frame_done().unwrap());
        assert_eq!(v.ack_frame().unwrap(), SR_IRQ_FRMCNT);
    }
}
