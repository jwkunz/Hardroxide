//! Zynq-7000 PL clock (FCLK0..3) configuration through the System Level
//! Control Registers (UG585 ch. 25/B.28). Register offsets are transcribed
//! from the TRM and UNVERIFIED on hardware.

use crate::FpgaError;
use hardroxide_mmio::Mmio;

/// Physical base and size of the SLCR window.
pub const SLCR_BASE: u64 = 0xF800_0000;
pub const SLCR_SIZE: usize = 0x1000;

const SLCR_LOCK: usize = 0x004;
const SLCR_UNLOCK: usize = 0x008;
const LOCK_KEY: u32 = 0x767B;
const UNLOCK_KEY: u32 = 0xDF0D;
const ARM_PLL_CTRL: usize = 0x100;
const DDR_PLL_CTRL: usize = 0x104;
const IO_PLL_CTRL: usize = 0x108;
const FPGA_CLK_CTRL: [usize; 4] = [0x170, 0x180, 0x190, 0x1A0];

fn pll_ctrl(srcsel: u32) -> usize {
    match srcsel {
        2 => ARM_PLL_CTRL,
        3 => DDR_PLL_CTRL,
        _ => IO_PLL_CTRL, // SRCSEL 0 and 1 are both IO PLL
    }
}

/// Pick `(div0, div1)` in `1..=63` minimising |src/(div0*div1) - target|;
/// ties prefer the smaller `div1`. Returns the divisors and achieved Hz.
pub fn best_divisors(src_hz: u64, target_hz: u64) -> (u32, u32, u64) {
    let mut best = (1, 1, src_hz);
    let mut best_err = u64::MAX;
    for d1 in 1..=63u32 {
        for d0 in 1..=63u32 {
            let hz = src_hz / (d0 as u64 * d1 as u64);
            let err = hz.abs_diff(target_hz);
            if err < best_err {
                best_err = err;
                best = (d0, d1, hz);
            }
        }
    }
    best
}

/// Current frequency of FCLK `index` computed from the SLCR registers
/// (source PLL multiplier and the two divisors). `Ok(0)` if the divisors are
/// zero (clock gated or unprogrammed). Read-only: needs no unlock.
pub fn read_pl_clock(slcr: &impl Mmio, ps_clk_hz: u64, index: u8) -> Result<u64, FpgaError> {
    let reg = *FPGA_CLK_CTRL
        .get(index as usize)
        .ok_or(FpgaError::BadClockIndex(index))?;
    let ctrl = slcr.read32(reg)?;
    let pll = slcr.read32(pll_ctrl((ctrl >> 4) & 0x3))?;
    let fdiv = ((pll >> 12) & 0x7f) as u64;
    let (d0, d1) = (((ctrl >> 8) & 0x3f) as u64, ((ctrl >> 20) & 0x3f) as u64);
    if d0 == 0 || d1 == 0 {
        return Ok(0);
    }
    Ok(ps_clk_hz * fdiv / (d0 * d1))
}

/// `ps_clk_hz` is the PS reference clock (50 MHz on PYNQ-Z2).
/// Set FCLK `index` to (approximately) `hz`, accepting up to 1% error.
/// Returns the achieved frequency.
pub fn set_pl_clock(
    slcr: &impl Mmio,
    ps_clk_hz: u64,
    index: u8,
    hz: u64,
) -> Result<u64, FpgaError> {
    let reg = *FPGA_CLK_CTRL
        .get(index as usize)
        .ok_or(FpgaError::BadClockIndex(index))?;
    let ctrl = slcr.read32(reg)?;
    let srcsel = (ctrl >> 4) & 0x3;
    let pll = slcr.read32(pll_ctrl(srcsel))?;
    let fdiv = (pll >> 12) & 0x7f;
    let bypass = pll & 0x10 != 0; // PLL_BYPASS_FORCE
    if fdiv == 0 || bypass {
        return Err(FpgaError::PllNotRunning(index));
    }
    let src_hz = ps_clk_hz * fdiv as u64;
    let (d0, d1, got) = best_divisors(src_hz, hz);
    if got.abs_diff(hz) * 100 > hz {
        return Err(FpgaError::ClockUnreachable {
            index,
            requested: hz,
            best: got,
            source_hz: src_hz,
        });
    }
    let new = (ctrl & !((0x3f << 8) | (0x3f << 20))) | (d0 << 8) | (d1 << 20);
    slcr.write32(SLCR_UNLOCK, UNLOCK_KEY)?;
    let r = slcr.write32(reg, new);
    slcr.write32(SLCR_LOCK, LOCK_KEY)?;
    r?;
    Ok(got)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn divisors() {
        let (d0, d1, hz) = best_divisors(999_999_990, 100_000_000);
        assert_eq!(d0 * d1, 10);
        assert!(hz.abs_diff(100_000_000) < 10);
    }

    #[test]
    fn programs_fclk0_from_io_pll() {
        let be = MockBackend::new();
        be.poke(SLCR_BASE + IO_PLL_CTRL as u64, 30 << 12); // 30 * 33.33 MHz = 1 GHz
        be.poke(SLCR_BASE + 0x170, 0x0010_0A00 | 0x1); // srcsel 0, other bits set
        let r = be.map(SLCR_BASE, SLCR_SIZE).unwrap();
        let got = set_pl_clock(&r, 33_333_333, 0, 100_000_000).unwrap();
        assert!(got.abs_diff(100_000_000) < 1000);
        let v = be.peek(SLCR_BASE + 0x170);
        assert_eq!(((v >> 8) & 0x3f) * ((v >> 20) & 0x3f), 10);
        assert_eq!(v & 0xf, 1, "unrelated bits preserved");
        assert_eq!(
            be.peek(SLCR_BASE + SLCR_LOCK as u64),
            LOCK_KEY,
            "left locked"
        );
    }

    #[test]
    fn read_back_matches_what_was_set() {
        let be = MockBackend::new();
        be.poke(SLCR_BASE + IO_PLL_CTRL as u64, 20 << 12); // 20 * 50 MHz = 1 GHz
        be.poke(SLCR_BASE + 0x170, 0x0010_0A00);
        let r = be.map(SLCR_BASE, SLCR_SIZE).unwrap();
        assert_eq!(read_pl_clock(&r, 50_000_000, 0).unwrap(), 100_000_000);
        let got = set_pl_clock(&r, 50_000_000, 0, 125_000_000).unwrap();
        assert_eq!(read_pl_clock(&r, 50_000_000, 0).unwrap(), got);
        assert_eq!(read_pl_clock(&r, 50_000_000, 1).unwrap(), 0); // never programmed
    }

    #[test]
    fn errors() {
        let be = MockBackend::new();
        let r = be.map(SLCR_BASE, SLCR_SIZE).unwrap();
        assert!(matches!(
            set_pl_clock(&r, 33_333_333, 4, 1),
            Err(FpgaError::BadClockIndex(4))
        ));
        assert!(matches!(
            set_pl_clock(&r, 33_333_333, 0, 1),
            Err(FpgaError::PllNotRunning(0))
        ));
        be.poke(SLCR_BASE + IO_PLL_CTRL as u64, 30 << 12);
        assert!(matches!(
            set_pl_clock(&r, 33_333_333, 0, 2_000_000_000),
            Err(FpgaError::ClockUnreachable { .. })
        ));
    }
}
