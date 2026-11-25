//! Clock frequencies derived from the PLL and clock-control registers (`CRL_APB`, `CRF_APB`).
//!
//! The PS must already have been initialised (by `psu_init` or an FSBL); this module only
//! reads what that set up. Frequencies are in Hz and assume the PS reference clock
//! (`ps_ref_hz`, 33.333 MHz on the RFSoC 4x2) given by the caller, which every PLL is taken
//! to run from (the reset choice and the vendor's).
//!
//! Every function takes both register windows: some clocks of one power domain come from a
//! PLL of the other.

use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

// CRL_APB.
const IOPLL: usize = 0x20;
const IOPLL_FRAC: usize = 0x28;
const RPLL: usize = 0x30;
const RPLL_FRAC: usize = 0x38;
const SDIO0_REF: usize = 0x6C;
const UART0_REF: usize = 0x74;
const I2C0_REF: usize = 0x120;
const TIMESTAMP_REF: usize = 0x128;
// CRF_APB.
const APLL: usize = 0x20;
const APLL_FRAC: usize = 0x28;
const DPLL: usize = 0x2C;
const DPLL_FRAC: usize = 0x34;
const VPLL: usize = 0x38;
const VPLL_FRAC: usize = 0x40;
const DPLL_TO_LPD: usize = 0x4C;
const VPLL_TO_LPD: usize = 0x50;
const ACPU: usize = 0x60;

/// The five PLLs: two in the low-power domain, three in the full-power domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pll {
    Io,
    Rpu,
    Apu,
    Ddr,
    Video,
}

/// Output of a PLL, in its own power domain: the reference times the feedback divider (plus
/// its fraction when enabled), halved if `DIV2` (bit 16) is set; or the reference itself
/// when the PLL is bypassed (bit 3).
pub fn pll_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, pll: Pll) -> Result<u64> {
    let (ctrl, frac) = match pll {
        Pll::Io => (crl.read32(IOPLL)?, crl.read32(IOPLL_FRAC)?),
        Pll::Rpu => (crl.read32(RPLL)?, crl.read32(RPLL_FRAC)?),
        Pll::Apu => (crf.read32(APLL)?, crf.read32(APLL_FRAC)?),
        Pll::Ddr => (crf.read32(DPLL)?, crf.read32(DPLL_FRAC)?),
        Pll::Video => (crf.read32(VPLL)?, crf.read32(VPLL_FRAC)?),
    };
    if ctrl & (1 << 3) != 0 {
        return Ok(ps_ref_hz);
    }
    let whole = u64::from((ctrl >> 8) & 0x7F);
    let fraction = if frac & (1 << 31) != 0 {
        u64::from(frac & 0xFFFF)
    } else {
        0
    };
    let vco = (ps_ref_hz * ((whole << 16) | fraction)) >> 16;
    Ok(vco >> ((ctrl >> 16) & 1))
}

/// A full-power PLL as the low-power domain receives it: through the `*_TO_LPD` divider.
fn to_lpd(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, pll: Pll) -> Result<u64> {
    let register = match pll {
        Pll::Ddr => DPLL_TO_LPD,
        _ => VPLL_TO_LPD,
    };
    let divisor = u64::from((crf.read32(register)? >> 8) & 0x3F).max(1);
    Ok(pll_hz(crl, crf, ps_ref_hz, pll)? / divisor)
}

/// A `*_REF_CTRL` clock of the low-power domain: the selected source divided by `DIVISOR0`
/// (bits 13:8) and `DIVISOR1` (bits 21:16). `third` is what source select 3 means for it.
fn peripheral_hz(
    crl: &impl Mmio,
    crf: &impl Mmio,
    ps_ref_hz: u64,
    register: usize,
    third: Pll,
) -> Result<u64> {
    let ctrl = crl.read32(register)?;
    let source = match ctrl & 0x7 {
        2 => pll_hz(crl, crf, ps_ref_hz, Pll::Rpu)?,
        3 => to_lpd(crl, crf, ps_ref_hz, third)?,
        _ => pll_hz(crl, crf, ps_ref_hz, Pll::Io)?,
    };
    let d0 = u64::from((ctrl >> 8) & 0x3F).max(1);
    let d1 = u64::from((ctrl >> 16) & 0x3F).max(1);
    Ok(source / d0 / d1)
}

/// The Cortex-A53 clock: the selected full-power PLL divided by `ACPU_CTRL.DIVISOR0`.
pub fn cpu_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64) -> Result<u64> {
    let ctrl = crf.read32(ACPU)?;
    let pll = match ctrl & 0x7 {
        2 => Pll::Ddr,
        3 => Pll::Video,
        _ => Pll::Apu,
    };
    let divisor = u64::from((ctrl >> 8) & 0x3F).max(1);
    Ok(pll_hz(crl, crf, ps_ref_hz, pll)? / divisor)
}

/// The reference clock of UART `n` (0 or 1).
pub fn uart_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64> {
    peripheral_hz(crl, crf, ps_ref_hz, UART0_REF + 4 * (n & 1), Pll::Ddr)
}

/// The base clock of SD host controller `n` (0 or 1).
pub fn sdio_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64> {
    peripheral_hz(crl, crf, ps_ref_hz, SDIO0_REF + 4 * (n & 1), Pll::Video)
}

/// The reference clock of I2C controller `n` (0 or 1).
pub fn i2c_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64> {
    peripheral_hz(crl, crf, ps_ref_hz, I2C0_REF + 4 * (n & 1), Pll::Ddr)
}

/// The clock of the system counter, which the processors' generic timers count.
pub fn timestamp_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64) -> Result<u64> {
    let ctrl = crl.read32(TIMESTAMP_REF)?;
    let source = match ctrl & 0x7 {
        0 => pll_hz(crl, crf, ps_ref_hz, Pll::Rpu)?,
        2 => pll_hz(crl, crf, ps_ref_hz, Pll::Io)?,
        3 => to_lpd(crl, crf, ps_ref_hz, Pll::Ddr)?,
        _ => ps_ref_hz, // 1xx: the PS reference itself
    };
    Ok(source / u64::from((ctrl >> 8) & 0x3F).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend, MockRegion};

    const REF: u64 = 33_333_333;

    /// The registers read on an RFSoC 4x2 after the fixtures' `psu_init`.
    fn board() -> (MockBackend, MockRegion, MockRegion) {
        let be = MockBackend::new();
        for (offset, value) in [
            (IOPLL, 0x0001_5A00),
            (RPLL, 0x0001_3F00),
            (SDIO0_REF, 0x0101_0800),
            (UART0_REF + 4, 0x0101_0F00),
            (I2C0_REF, 0x0101_0F00),
            (TIMESTAMP_REF, 0x0100_0104),
        ] {
            be.poke(0xFF5E_0000 + offset as u64, value);
        }
        for (offset, value) in [
            (APLL, 0x0001_4800),
            (DPLL, 0x0001_4800),
            (VPLL, 0x0001_5A00),
            (DPLL_TO_LPD, 0x0300),
            (VPLL_TO_LPD, 0x0400),
            (ACPU, 0x0300_0100),
        ] {
            be.poke(0xFD1A_0000 + offset as u64, value);
        }
        let crl = be.map(0xFF5E_0000, 0x1000).unwrap();
        let crf = be.map(0xFD1A_0000, 0x100).unwrap();
        (be, crl, crf)
    }

    fn near(got: u64, want: u64) {
        assert!(got.abs_diff(want) <= want / 100_000, "{got} not {want}");
    }

    #[test]
    fn matches_what_vivado_reports_for_the_board() {
        // The values are the design's ACT_FREQMHZ parameters.
        let (_be, crl, crf) = board();
        near(pll_hz(&crl, &crf, REF, Pll::Io).unwrap(), 1_500_000_000);
        near(cpu_hz(&crl, &crf, REF).unwrap(), 1_199_999_756);
        near(uart_ref_hz(&crl, &crf, REF, 1).unwrap(), 99_999_985);
        near(sdio_ref_hz(&crl, &crf, REF, 0).unwrap(), 187_499_969);
        near(i2c_ref_hz(&crl, &crf, REF, 0).unwrap(), 99_999_985);
        near(timestamp_hz(&crl, &crf, REF).unwrap(), 33_333_328);
    }

    #[test]
    fn bypass_fraction_and_cross_domain_sources() {
        let (be, crl, crf) = board();
        be.poke(0xFF5E_0000 + IOPLL as u64, 0x0001_5A08);
        assert_eq!(pll_hz(&crl, &crf, REF, Pll::Io).unwrap(), REF);
        // Half a step of the feedback divider, before the halving.
        be.poke(0xFF5E_0000 + RPLL_FRAC as u64, 0x8000_8000);
        near(
            pll_hz(&crl, &crf, REF, Pll::Rpu).unwrap(),
            REF * 127 / 2 / 2,
        );
        // UART 0 from the DDR PLL through its divider to the low-power domain: 1200 / 3 / 4.
        be.poke(0xFF5E_0000 + UART0_REF as u64, 0x0101_0403);
        near(uart_ref_hz(&crl, &crf, REF, 0).unwrap(), 100_000_000);
    }
}
