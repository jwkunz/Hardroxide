//! The PYNQ audio codec controller (`xilinx.com:user:audio_codec_ctrl`): an I2S serialiser and
//! deserialiser for the ADAU1761 codec with an AXI-Lite register window and **no FIFO and no
//! interrupt**: one stereo sample in each direction, so software has to move a sample every
//! sample period (about 20.5 us).
//!
//! The register map and the timing come from the core's VHDL source in the PYNQ repository
//! (Apache-2.0): the bit clock is the AXI clock divided by 32 and the left/right clock by 2048
//! (3.125 MHz and 48.828 kHz with a 100 MHz AXI clock). Samples are 24 bits in the low bits of a
//! 32-bit word. The codec itself is configured over I2C (see `hardroxide-audio`).

use hardroxide_mmio::{Mmio, MmioError};

const RX_L: usize = 0x00;
const RX_R: usize = 0x04;
const TX_L: usize = 0x08;
const TX_R: usize = 0x0C;
const STATUS: usize = 0x10;

/// The sample rate of the controller with a 100 MHz AXI clock, in hertz (`100 MHz / 2048`).
pub const SAMPLE_RATE_HZ: f64 = 100_000_000.0 / 2048.0;

/// A stereo sample: 24-bit signed values, sign-extended in an `i32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Frame {
    pub left: i32,
    pub right: i32,
}

impl Frame {
    /// From 16-bit samples.
    pub fn from_i16(left: i16, right: i16) -> Frame {
        Frame {
            left: i32::from(left) << 8,
            right: i32::from(right) << 8,
        }
    }

    /// To 16-bit samples (the top 16 of the 24 bits).
    pub fn to_i16(self) -> (i16, i16) {
        ((self.left >> 8) as i16, (self.right >> 8) as i16)
    }
}

fn sign_extend_24(v: u32) -> i32 {
    ((v << 8) as i32) >> 8
}

pub struct AudioCodecCtrl<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AudioCodecCtrl<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Whether a new input sample has arrived since the last [`AudioCodecCtrl::clear_ready`].
    pub fn ready(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(STATUS)? & 1 != 0)
    }

    /// Clear the ready flag (a write to the status register); do it, then wait for the next sample.
    pub fn clear_ready(&self) -> Result<(), MmioError> {
        self.regs.write32(STATUS, 1)
    }

    /// The last received frame.
    pub fn read_frame(&self) -> Result<Frame, MmioError> {
        Ok(Frame {
            left: sign_extend_24(self.regs.read32(RX_L)?),
            right: sign_extend_24(self.regs.read32(RX_R)?),
        })
    }

    /// Set the frame to transmit at the next sample time.
    pub fn write_frame(&self, f: Frame) -> Result<(), MmioError> {
        self.regs.write32(TX_L, (f.left as u32) & 0xFF_FFFF)?;
        self.regs.write32(TX_R, (f.right as u32) & 0xFF_FFFF)
    }

    /// Wait for the next sample time by polling (spin), up to `spins` reads; `false` if none came.
    pub fn wait_sample(&self, spins: u32) -> Result<bool, MmioError> {
        self.clear_ready()?;
        for _ in 0..spins {
            if self.ready()? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const BASE: u64 = 0x43C0_0000;

    #[test]
    fn registers_and_sign_extension() {
        let be = MockBackend::new();
        let a = AudioCodecCtrl::new(be.map(BASE, 0x1000).unwrap());
        be.poke(BASE + RX_L as u64, 0x00FF_FFFE); // -2 as 24 bits
        be.poke(BASE + RX_R as u64, 0x0000_0102);
        assert_eq!(
            a.read_frame().unwrap(),
            Frame {
                left: -2,
                right: 0x102
            }
        );
        a.write_frame(Frame { left: -1, right: 5 }).unwrap();
        assert_eq!(be.peek(BASE + TX_L as u64), 0x00FF_FFFF);
        assert_eq!(be.peek(BASE + TX_R as u64), 5);
        a.clear_ready().unwrap();
        assert_eq!(be.peek(BASE + STATUS as u64), 1);
    }

    #[test]
    fn sixteen_bit_conversions_round_trip() {
        for (l, r) in [(0i16, 0i16), (1, -1), (i16::MAX, i16::MIN), (-1234, 4321)] {
            assert_eq!(Frame::from_i16(l, r).to_i16(), (l, r));
        }
    }

    #[test]
    fn the_sample_rate_is_the_lrclk_divide() {
        assert!((SAMPLE_RATE_HZ - 48_828.125).abs() < 1e-9);
    }
}
