//! Digilent `axi_dynclk` (`digilentinc.com:ip:axi_dynclk`): a clock generator whose MMCM is
//! reconfigured from software, used for the pixel clock of the PYNQ base overlay's HDMI output.
//!
//! The register map and the MMCM reconfiguration (divider encoding, lock and filter tables from
//! Xilinx application note XAPP888) follow Digilent's `axi_dynclk` driver (MIT licence,
//! <https://github.com/Digilent/vivado-library>), which is the only description of this core; the
//! code here is written for this crate. The output goes through a divide-by-five buffer, so the MMCM
//! runs at five times the requested pixel clock.

use core::time::Duration;
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{Mmio, MmioError};

const CTRL: usize = 0x0;
const STATUS: usize = 0x4;
const CLK_L: usize = 0x8;
const FB_L: usize = 0xC;
const FB_H_CLK_H: usize = 0x10;
const DIV: usize = 0x14;
const LOCK_L: usize = 0x18;
const FLTR_LOCK_H: usize = 0x1C;
const REF_CLK_FREQ: usize = 0x20;

const BIT_WEDGE: u32 = 13;
const BIT_NOCOUNT: u32 = 12;
const ERR_DIVIDER: u32 = 1 << BIT_WEDGE | 1 << BIT_NOCOUNT;
const DIV_1_REGMASK: u32 = 0x1041;

// MMCM limits in kHz (7-series, speed grade -1), and divider ranges.
const VCO_MIN: u32 = 600_000;
const VCO_MAX: u32 = 1_200_000;
const PFD_MIN: u32 = 10_000;
const PFD_MAX: u32 = 450_000;
const OUT_MIN: u32 = 4_000;
const OUT_MAX: u32 = 800_000;
const DIV_MAX: u32 = 106;
const FB_MIN: u32 = 2;
const FB_MAX: u32 = 64;
const CLKDIV_MAX: u32 = 128;

static LOCK_LOOKUP: [u64; 64] = [
    0x31BE8FA401,
    0x31BE8FA401,
    0x423E8FA401,
    0x5AFE8FA401,
    0x73BE8FA401,
    0x8C7E8FA401,
    0x9CFE8FA401,
    0xB5BE8FA401,
    0xCE7E8FA401,
    0xE73E8FA401,
    0xFFF84FA401,
    0xFFF39FA401,
    0xFFEEEFA401,
    0xFFEBCFA401,
    0xFFE8AFA401,
    0xFFE71FA401,
    0xFFE3FFA401,
    0xFFE26FA401,
    0xFFE0DFA401,
    0xFFDF4FA401,
    0xFFDDBFA401,
    0xFFDC2FA401,
    0xFFDA9FA401,
    0xFFD90FA401,
    0xFFD90FA401,
    0xFFD77FA401,
    0xFFD5EFA401,
    0xFFD5EFA401,
    0xFFD45FA401,
    0xFFD45FA401,
    0xFFD2CFA401,
    0xFFD2CFA401,
    0xFFD2CFA401,
    0xFFD13FA401,
    0xFFD13FA401,
    0xFFD13FA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
    0xFFCFAFA401,
];
static FILTER_LOOKUP_LOW: [u32; 64] = [
    0x05F, 0x057, 0x07B, 0x05B, 0x06B, 0x073, 0x073, 0x073, 0x073, 0x04B, 0x04B, 0x04B, 0x0B3,
    0x053, 0x053, 0x053, 0x053, 0x053, 0x053, 0x053, 0x053, 0x053, 0x053, 0x063, 0x063, 0x063,
    0x063, 0x063, 0x063, 0x063, 0x063, 0x063, 0x063, 0x063, 0x063, 0x063, 0x063, 0x093, 0x093,
    0x093, 0x093, 0x093, 0x093, 0x093, 0x093, 0x093, 0x093, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3,
    0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3, 0x0A3,
];

/// MMCM settings for an output frequency: input divide, feedback multiply, output divide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    /// Output frequency of the MMCM in kHz (before the divide-by-five buffer).
    pub freq_khz: u32,
    pub fb_mult: u32,
    pub clk_div: u32,
    pub main_div: u32,
}

/// The six words written to the core to apply a [`Mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regs {
    pub clk0_l: u32,
    pub fb_l: u32,
    pub fb_h_clk0_h: u32,
    pub div: u32,
    pub lock_l: u32,
    pub fltr_lock_h: u32,
}

/// High/low time encoding of a divider (`None` outside 1..=128).
fn divider(divide: u32) -> Option<u32> {
    if !(1..=128).contains(&divide) {
        return None;
    }
    if divide == 1 {
        return Some(DIV_1_REGMASK);
    }
    let high = divide / 2;
    let (low, mut out) = if divide & 1 == 1 {
        (high + 1, 1 << BIT_WEDGE)
    } else {
        (high, 0)
    };
    out |= 0x03F & low;
    out |= 0xFC0 & (high << 6);
    debug_assert_ne!(out, ERR_DIVIDER);
    Some(out)
}

fn count_calc(divide: u32) -> Option<u32> {
    let d = divider(divide)?;
    Some((0xFFF & d) | ((d << 10) & 0x00C0_0000))
}

/// Registers for `mode`; `None` if a divider is out of range.
pub fn registers_for(mode: &Mode) -> Option<Regs> {
    if !(FB_MIN..=FB_MAX).contains(&mode.fb_mult) {
        return None;
    }
    let lock = LOCK_LOOKUP[(mode.fb_mult - 1) as usize];
    Some(Regs {
        clk0_l: count_calc(mode.clk_div)?,
        fb_l: count_calc(mode.fb_mult)?,
        fb_h_clk0_h: 0,
        div: divider(mode.main_div)?,
        lock_l: (lock & 0xFFFF_FFFF) as u32,
        fltr_lock_h: ((lock >> 32) & 0xFF) as u32
            | ((FILTER_LOOKUP_LOW[(mode.fb_mult - 1) as usize] << 16) & 0x03FF_0000),
    })
}

/// The MMCM settings closest to `freq_khz` (the MMCM output, kHz) from a reference of
/// `parent_khz`, searching input dividers upward from the smallest valid one; `None` if the
/// reference is zero or nothing fits.
pub fn find_mode(freq_khz: u32, parent_khz: u32) -> Option<Mode> {
    if parent_khz == 0 {
        return None;
    }
    let freq = freq_khz.clamp(OUT_MIN, OUT_MAX);
    let mut cur_div = if parent_khz > PFD_MAX { 2 } else { 1 };
    let max_div = (parent_khz / PFD_MIN).min(DIV_MAX);
    let mut best: Option<(u32, Mode)> = None;
    while cur_div <= max_div {
        let min_fb = (cur_div * VCO_MIN.div_ceil(parent_khz)).max(FB_MIN);
        let max_fb = (cur_div * (VCO_MAX / parent_khz)).min(FB_MAX);
        let div_val = cur_div * freq;
        let clk_mult = (u64::from(parent_khz) * 1000 + u64::from(div_val) / 2) / u64::from(div_val);
        let mut fb = min_fb;
        while fb <= max_fb {
            let clk_div = (((clk_mult * u64::from(fb) + 500) / 1000) as u32).clamp(1, CLKDIV_MAX);
            let cur = parent_khz * fb / cur_div / clk_div;
            let err = cur.abs_diff(freq);
            if best.is_none_or(|(e, _)| err < e) {
                best = Some((
                    err,
                    Mode {
                        freq_khz: cur,
                        fb_mult: fb,
                        clk_div,
                        main_div: cur_div,
                    },
                ));
            }
            if err == 0 {
                return best.map(|(_, m)| m);
            }
            fb += 1;
        }
        cur_div += 1;
    }
    best.map(|(_, m)| m)
}

/// Errors from the clock generator.
#[derive(Debug, thiserror::Error)]
pub enum DynclkError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    /// The core reports a zero reference clock.
    #[error("the clock generator reports no reference clock")]
    NoReference,
    /// No MMCM setting produces the frequency.
    #[error("no MMCM setting produces {0} Hz")]
    Unreachable(u32),
    /// The MMCM did not lock in time.
    #[error("the MMCM did not lock")]
    NoLock,
}

/// The reconfigurable clock generator.
pub struct AxiDynclk<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiDynclk<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Frequency of the reference clock the core reports (Hz).
    pub fn ref_clk_hz(&self) -> Result<u32, MmioError> {
        self.regs.read32(REF_CLK_FREQ)
    }

    /// Whether the MMCM is locked and the output enabled.
    pub fn locked(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(STATUS)? != 0)
    }

    /// Switch the output off.
    pub fn disable(&self) -> Result<(), MmioError> {
        self.regs.write32(CTRL, 0)
    }

    /// Program the output to about `pixel_hz` (the generator drives a divide-by-five buffer, so the MMCM
    /// is set to five times that), switch it off and on, and wait for the lock. Returns the pixel
    /// frequency actually produced, in Hz.
    pub fn set_rate(&self, pixel_hz: u32) -> Result<u32, DynclkError> {
        let parent = self.ref_clk_hz()?;
        if parent == 0 {
            return Err(DynclkError::NoReference);
        }
        // Hz -> kHz at five times the pixel rate; the reference in kHz.
        let want = (pixel_hz + 100) / 200;
        let mode =
            find_mode(want, (parent + 500) / 1000).ok_or(DynclkError::Unreachable(pixel_hz))?;
        let r = registers_for(&mode).ok_or(DynclkError::Unreachable(pixel_hz))?;
        self.regs.write32(CLK_L, r.clk0_l)?;
        self.regs.write32(FB_L, r.fb_l)?;
        self.regs.write32(FB_H_CLK_H, r.fb_h_clk0_h)?;
        self.regs.write32(DIV, r.div)?;
        self.regs.write32(LOCK_L, r.lock_l)?;
        self.regs.write32(FLTR_LOCK_H, r.fltr_lock_h)?;
        self.regs.write32(CTRL, 0)?;
        self.regs.write32(CTRL, 1)?;
        let t0 = Instant::now();
        while !self.locked()? {
            if t0.elapsed() > Duration::from_millis(100) {
                return Err(DynclkError::NoLock);
            }
            sleep(Duration::from_micros(100));
        }
        Ok(mode.freq_khz * 200)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn dividers_encode_high_and_low_times() {
        assert_eq!(divider(1), Some(0x1041));
        // 10: five high, five low.
        assert_eq!(divider(10), Some(5 | 5 << 6));
        // 5: high 2, low 3 and the odd-divide edge bit.
        assert_eq!(divider(5), Some(3 | 2 << 6 | 1 << 13));
        assert_eq!(divider(0), None);
        assert_eq!(divider(129), None);
    }

    #[test]
    fn a_74_25_mhz_pixel_clock_from_100_mhz() {
        {
            // Five times 74.25 MHz = 371.25 MHz from a 100 MHz reference.
            let m = find_mode(74_250 * 5, 100_000).unwrap();
            assert!(m.freq_khz.abs_diff(371_250) <= 400, "{m:?}"); // within 0.1 %
            // The settings obey the MMCM's limits.
            let vco = 100_000 * m.fb_mult / m.main_div;
            assert!((600_000..=1_200_000).contains(&vco), "{m:?}");
            assert!(registers_for(&m).is_some());
        }
    }

    #[test]
    fn standard_pixel_clocks_are_reachable() {
        {
            for (hz, tol) in [
                (25_175_000u32, 0.003f64),
                (40_000_000, 0.001),
                (74_250_000, 0.001),
                (108_000_000, 0.001),
                (148_500_000, 0.001),
            ] {
                let m = find_mode((hz + 100) / 200, 100_000).unwrap();
                let got = f64::from(m.freq_khz) * 200.0;
                assert!(
                    ((got - f64::from(hz)) / f64::from(hz)).abs() < tol,
                    "{hz}: {m:?} -> {got}"
                );
            }
        }
    }

    #[test]
    fn set_rate_writes_the_six_words_then_enables() {
        {
            let be = MockBackend::new();
            let r = be.map(0x43C1_0000, 0x1000).unwrap();
            be.poke(0x43C1_0000 + REF_CLK_FREQ as u64, 100_000_000);
            be.poke(0x43C1_0000 + STATUS as u64, 1);
            let d = AxiDynclk::new(r);
            let hz = d.set_rate(74_250_000).unwrap();
            assert!(hz.abs_diff(74_250_000) < 100_000, "{hz}");
            assert_eq!(be.peek(0x43C1_0000 + CTRL as u64), 1);
            assert_ne!(be.peek(0x43C1_0000 + CLK_L as u64), 0);
            assert_ne!(be.peek(0x43C1_0000 + FB_L as u64), 0);
            assert_ne!(be.peek(0x43C1_0000 + FLTR_LOCK_H as u64), 0);
        }
    }

    #[test]
    fn registers_match_the_digilent_driver_for_one_case() {
        {
            // fb 37, out divide 10, input divide 1 (a 100 MHz reference to 370 MHz).
            let r = registers_for(&Mode {
                freq_khz: 370_000,
                fb_mult: 37,
                clk_div: 10,
                main_div: 1,
            })
            .unwrap();
            assert_eq!(r.div, 0x1041);
            assert_eq!(r.clk0_l, 5 | 5 << 6);
            // 37 is odd: low 19, high 18; the odd-divide edge bit moves to bit 23.
            assert_eq!(r.fb_l, (19 | 18 << 6) | 0x80_0000);
        }
    }
}
