//! HDMI output on the PYNQ base overlay.
//!
//! The overlay's output pipeline is: frame buffers in memory, an AXI VDMA read channel, a pixel
//! format unpacker (`pixel_unpack`), a colour converter (`color_convert`), the HDMI transmitter,
//! with a video timing generator (`v_tc`) pacing the stream and a reconfigurable clock
//! (`axi_dynclk`) providing the pixel clock. [`Display`] brings all of it up for a [`VideoMode`]
//! and switches the DMA between frame buffers for double buffering.
//!
//! Pixels are 32 bits, `0x00RRGGBB`, four bytes per pixel in memory ([`BYTES_PER_PIXEL`]).
//!
//! The steps follow PYNQ's video pipeline (Apache-2.0) for the colour and pixel-format settings
//! and the vendor's drivers for the video timing controller; none of this has run on a board
//! yet when this version of the crate was written (see the project's verification notes).

#![cfg_attr(not(any(feature = "std", test)), no_std)]

use core::time::Duration;
use hardroxide_ip::dynclk::{AxiDynclk, DynclkError};
use hardroxide_ip::vdma::{AxiVdma, Geometry, VdmaError};
use hardroxide_ip::vtc::{Timing, VideoTimingController};
use hardroxide_mmio::{Mmio, MmioError};

/// Bytes per pixel in the frame buffers.
pub const BYTES_PER_PIXEL: u32 = 4;

/// A display mode: size, refresh rate, pixel clock and timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoMode {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub pixel_clock_hz: u32,
    pub timing: Timing,
}

impl VideoMode {
    /// Bytes in one line of a frame buffer.
    pub const fn bytes_per_line(&self) -> u32 {
        self.width * BYTES_PER_PIXEL
    }

    /// Bytes in a frame buffer.
    pub const fn frame_bytes(&self) -> u32 {
        self.bytes_per_line() * self.height
    }

    /// Pixels in a frame buffer.
    pub const fn frame_pixels(&self) -> usize {
        (self.width * self.height) as usize
    }
}

const fn timing(
    h: (u32, u32, u32, u32),
    v: (u32, u32, u32, u32),
    h_positive: bool,
    v_positive: bool,
) -> Timing {
    Timing {
        h_active: h.0,
        h_front_porch: h.1,
        h_sync: h.2,
        h_back_porch: h.3,
        v_active: v.0,
        v_front_porch: v.1,
        v_sync: v.2,
        v_back_porch: v.3,
        h_sync_positive: h_positive,
        v_sync_positive: v_positive,
    }
}

/// VGA, 640x480 at 60 Hz (VESA).
pub const VGA_640X480_60: VideoMode = VideoMode {
    name: "640x480@60",
    width: 640,
    height: 480,
    refresh_hz: 60,
    pixel_clock_hz: 25_175_000,
    timing: timing((640, 16, 96, 48), (480, 10, 2, 33), false, false),
};

/// SVGA, 800x600 at 60 Hz (VESA).
pub const SVGA_800X600_60: VideoMode = VideoMode {
    name: "800x600@60",
    width: 800,
    height: 600,
    refresh_hz: 60,
    pixel_clock_hz: 40_000_000,
    timing: timing((800, 40, 128, 88), (600, 1, 4, 23), true, true),
};

/// XGA, 1024x768 at 60 Hz (VESA).
pub const XGA_1024X768_60: VideoMode = VideoMode {
    name: "1024x768@60",
    width: 1024,
    height: 768,
    refresh_hz: 60,
    pixel_clock_hz: 65_000_000,
    timing: timing((1024, 24, 136, 160), (768, 3, 6, 29), false, false),
};

/// 720p, 1280x720 at 60 Hz (CEA-861).
pub const HD_1280X720_60: VideoMode = VideoMode {
    name: "1280x720@60",
    width: 1280,
    height: 720,
    refresh_hz: 60,
    pixel_clock_hz: 74_250_000,
    timing: timing((1280, 110, 40, 220), (720, 5, 5, 20), true, true),
};

/// SXGA, 1280x1024 at 60 Hz (VESA).
pub const SXGA_1280X1024_60: VideoMode = VideoMode {
    name: "1280x1024@60",
    width: 1280,
    height: 1024,
    refresh_hz: 60,
    pixel_clock_hz: 108_000_000,
    timing: timing((1280, 48, 112, 248), (1024, 1, 3, 38), true, true),
};

/// 1080p, 1920x1080 at 60 Hz (CEA-861).
pub const FHD_1920X1080_60: VideoMode = VideoMode {
    name: "1920x1080@60",
    width: 1920,
    height: 1080,
    refresh_hz: 60,
    pixel_clock_hz: 148_500_000,
    timing: timing((1920, 88, 44, 148), (1080, 4, 5, 36), true, true),
};

/// Every mode this crate knows.
pub const MODES: [VideoMode; 6] = [
    VGA_640X480_60,
    SVGA_800X600_60,
    XGA_1024X768_60,
    HD_1280X720_60,
    SXGA_1280X1024_60,
    FHD_1920X1080_60,
];

/// The mode with this size, if there is one.
pub fn mode_for(width: u32, height: u32) -> Option<&'static VideoMode> {
    MODES
        .iter()
        .find(|m| m.width == width && m.height == height)
}

/// Colour conversion between the frame buffer's pixels and the transmitter's: a 3x4 matrix of
/// 10-bit signed fixed-point values (2 integer bits, 8 fractional) as the converter
/// takes them, `out = M * in + bias`.
pub mod colour {
    /// `[c1 c2 c3 / c4 c5 c6 / c7 c8 c9 / bias1 bias2 bias3]`.
    pub type Matrix = [f32; 12];

    /// Pass the stream through unchanged: byte 0 of a pixel is blue, then green, then red, which is
    /// `0x00RRGGBB` in a little-endian word.
    pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

    /// Swap red and blue (for buffers laid out `0x00BBGGRR`).
    pub const SWAP_RB: Matrix = [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];

    /// The register value for one coefficient.
    pub fn encode(c: f32) -> u32 {
        // Truncated to 10 bits, two's complement, as the core reads it.
        ((c * 256.0) as i32 as u32) & 0x3FF
    }
}

/// Offset of the first colour-converter coefficient; each takes 8 bytes (a register and a gap).
const COLOUR_COEFF_BASE: usize = 0x10;
/// The pixel unpacker's mode register: 0 = 24 bits per pixel, 1 = 32, 2 = 8.
const UNPACK_MODE: usize = 0x10;
const UNPACK_32_BPP: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error(transparent)]
    Clock(#[from] DynclkError),
    #[error(transparent)]
    Vdma(#[from] VdmaError),
    #[error("{0} frame buffers given; the display needs between 1 and {max}", max = hardroxide_ip::vdma::MAX_FRAMES)]
    Frames(usize),
}

/// The output pipeline of the base overlay.
pub struct Display<'a, R: Mmio> {
    pub dynclk: &'a AxiDynclk<R>,
    pub vtc: &'a VideoTimingController<R>,
    pub vdma: &'a AxiVdma<R>,
    /// The output colour converter's registers (`video_hdmi_out_color_convert`).
    pub colour: &'a R,
    /// The output pixel unpacker's registers (`video_hdmi_out_pixel_unpack`).
    pub unpack: &'a R,
}

impl<R: Mmio> Display<'_, R> {
    /// Set the colour conversion matrix (can be changed while running).
    pub fn set_colour(&self, m: &colour::Matrix) -> Result<(), MmioError> {
        for (i, c) in m.iter().enumerate() {
            self.colour
                .write32(COLOUR_COEFF_BASE + 8 * i, colour::encode(*c))?;
        }
        Ok(())
    }

    /// Bring the output up in `mode`, showing `frames` (physical addresses of frame buffers, each
    /// at least [`VideoMode::frame_bytes`] long, 32-bit addresses) from the first. Returns the pixel
    /// clock actually generated, in Hz.
    pub fn start(&self, mode: &VideoMode, frames: &[u32]) -> Result<u32, DisplayError> {
        if frames.is_empty() || frames.len() > hardroxide_ip::vdma::MAX_FRAMES {
            return Err(DisplayError::Frames(frames.len()));
        }
        self.vtc.stop()?;
        let pixel_hz = self.dynclk.set_rate(mode.pixel_clock_hz)?;
        self.set_colour(&colour::IDENTITY)?;
        self.unpack.write32(UNPACK_MODE, UNPACK_32_BPP)?;
        self.vdma.start_read(
            frames,
            &Geometry {
                hsize: mode.bytes_per_line(),
                stride: mode.bytes_per_line(),
                lines: mode.height,
            },
        )?;
        // The timing generator starts last: until it runs the stream is held back, so the DMA waits.
        self.vtc.start(&mode.timing)?;
        Ok(pixel_hz)
    }

    /// Stop the timing generator and the DMA.
    pub fn stop(&self) -> Result<(), MmioError> {
        self.vtc.stop()?;
        self.vdma.stop_read()
    }

    /// Show frame buffer `n` from the next frame on.
    pub fn show(&self, n: u32) -> Result<(), MmioError> {
        self.vdma.show_frame(n)
    }

    /// The frame buffer being scanned out now.
    pub fn showing(&self) -> Result<u32, MmioError> {
        self.vdma.current_frame()
    }

    /// Wait for the next frame boundary (`false` if it did not come within `timeout`).
    pub fn wait_frame(&self, timeout: Duration) -> Result<bool, MmioError> {
        self.vdma.wait_frame(timeout)
    }

    /// Check the DMA for errors.
    pub fn check(&self) -> Result<(), VdmaError> {
        self.vdma.check_read()
    }
}

/// View a frame buffer's bytes as pixels. `None` if the buffer is not 4-byte aligned or not a
/// whole number of pixels.
pub fn as_pixels(bytes: &mut [u8]) -> Option<&mut [u32]> {
    // SAFETY: every bit pattern is a valid `u32`; `align_to_mut` only returns the aligned middle.
    let (head, mid, tail) = unsafe { bytes.align_to_mut::<u32>() };
    (head.is_empty() && tail.is_empty()).then_some(mid)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn modes_have_consistent_timings() {
        for m in MODES {
            assert_eq!(m.timing.h_active, m.width, "{}", m.name);
            assert_eq!(m.timing.v_active, m.height, "{}", m.name);
            // Pixel clock = total pixels per frame x refresh rate, to within 1 % (the refresh rates are rounded).
            let total = u64::from(m.timing.h_total()) * u64::from(m.timing.v_total());
            let hz = total * u64::from(m.refresh_hz);
            let err = hz.abs_diff(u64::from(m.pixel_clock_hz)) as f64 / f64::from(m.pixel_clock_hz);
            assert!(err < 0.01, "{}: {hz} vs {}", m.name, m.pixel_clock_hz);
        }
    }

    #[test]
    fn well_known_totals() {
        assert_eq!(
            (
                HD_1280X720_60.timing.h_total(),
                HD_1280X720_60.timing.v_total()
            ),
            (1650, 750)
        );
        assert_eq!(
            (
                FHD_1920X1080_60.timing.h_total(),
                FHD_1920X1080_60.timing.v_total()
            ),
            (2200, 1125)
        );
        assert_eq!(
            (
                VGA_640X480_60.timing.h_total(),
                VGA_640X480_60.timing.v_total()
            ),
            (800, 525)
        );
        assert_eq!(HD_1280X720_60.frame_bytes(), 1280 * 720 * 4);
        assert_eq!(mode_for(800, 600).map(|m| m.name), Some("800x600@60"));
        assert!(mode_for(1, 1).is_none());
    }

    #[test]
    fn coefficients_are_ten_bit_fixed_point() {
        assert_eq!(colour::encode(1.0), 0x100);
        assert_eq!(colour::encode(0.0), 0);
        assert_eq!(colour::encode(-1.0), 0x300);
        assert_eq!(colour::encode(0.5), 0x80);
    }

    #[test]
    fn start_programs_every_stage_and_the_timing_generator_last() {
        let be = MockBackend::new();
        let (dc, vt, vd, cc, up) = (
            0x43C1_0000u64,
            0x43C2_0000u64,
            0x4300_0000u64,
            0x43C6_0000u64,
            0x43C7_0000u64,
        );
        be.poke(dc + 0x20, 100_000_000); // reference clock the core reports
        be.poke(dc + 0x4, 1); // locked
        let dynclk = AxiDynclk::new(be.map(dc, 0x1000).unwrap());
        let vtc = VideoTimingController::new(be.map(vt, 0x1000).unwrap());
        let vdma = AxiVdma::new(be.map(vd, 0x1000).unwrap());
        let (colour_regs, unpack_regs) = (be.map(cc, 0x1000).unwrap(), be.map(up, 0x1000).unwrap());
        let d = Display {
            dynclk: &dynclk,
            vtc: &vtc,
            vdma: &vdma,
            colour: &colour_regs,
            unpack: &unpack_regs,
        };
        // The mock cannot clear the VDMA's self-clearing reset bit, so only the parts before it are checked.
        let r = d.start(&HD_1280X720_60, &[0x1000_0000, 0x1040_0000]);
        assert!(
            matches!(r, Err(DisplayError::Vdma(VdmaError::ResetTimeout))),
            "{r:?}"
        );
        assert_eq!(be.peek(cc + 0x10), 0x100); // c1 = 1.0
        assert_eq!(be.peek(cc + 0x10 + 8), 0); // c2 = 0
        assert_eq!(be.peek(cc + 0x10 + 8 * 4), 0x100); // c5 = 1.0
        assert_eq!(be.peek(up + 0x10), 1); // 32 bits per pixel
        assert_eq!(be.peek(dc), 1); // the clock is enabled
        assert!(matches!(
            d.start(&HD_1280X720_60, &[]),
            Err(DisplayError::Frames(0))
        ));
    }

    #[test]
    fn frame_buffers_view_as_pixels() {
        let mut v = [0u32; 4];
        let bytes = unsafe { core::slice::from_raw_parts_mut(v.as_mut_ptr().cast::<u8>(), 16) };
        assert_eq!(as_pixels(bytes).map(|p| p.len()), Some(4));
        let mut odd = [0u8; 7];
        assert!(as_pixels(&mut odd).is_none() || odd.len() % 4 == 0);
    }
}
