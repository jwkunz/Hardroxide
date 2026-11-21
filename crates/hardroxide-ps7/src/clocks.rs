//! Clock frequencies derived from the PLL and clock-control registers in the SLCR.
//!
//! The PS must already have been initialised (by `ps7_init` or an FSBL); this module only
//! reads what that set up. Frequencies are in Hz and assume the PS reference clock
//! (`ps_clk_hz`, 50 MHz on the PYNQ-Z2) given by the caller.

use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

const ARM_PLL_CTRL: usize = 0x100;
const DDR_PLL_CTRL: usize = 0x104;
const IO_PLL_CTRL: usize = 0x108;
const ARM_CLK_CTRL: usize = 0x120;
const UART_CLK_CTRL: usize = 0x154;

/// Which PLL feeds a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pll {
    Arm,
    Ddr,
    Io,
}

/// Output of a PLL: `ps_clk * feedback divider`, or the reference itself when the PLL is
/// bypassed (`PLL_BYPASS_FORCE`, bit 4).
pub fn pll_hz(slcr: &impl Mmio, ps_clk_hz: u64, pll: Pll) -> Result<u64> {
    let ctrl = slcr.read32(match pll {
        Pll::Arm => ARM_PLL_CTRL,
        Pll::Ddr => DDR_PLL_CTRL,
        Pll::Io => IO_PLL_CTRL,
    })?;
    if ctrl & (1 << 4) != 0 {
        return Ok(ps_clk_hz);
    }
    Ok(ps_clk_hz * u64::from((ctrl >> 12) & 0x7F))
}

/// The CPU core clock (`CPU_6x4x`): the selected PLL divided by `ARM_CLK_CTRL.DIVISOR`.
pub fn cpu_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64> {
    let ctrl = slcr.read32(ARM_CLK_CTRL)?;
    let pll = match (ctrl >> 4) & 0x3 {
        2 => Pll::Ddr,
        3 => Pll::Io,
        _ => Pll::Arm,
    };
    let div = u64::from((ctrl >> 8) & 0x3F).max(1);
    Ok(pll_hz(slcr, ps_clk_hz, pll)? / div)
}

/// The clock of the MPCore private peripherals (global timer, private timer, watchdog):
/// half the CPU clock (`CPU_3x2x`).
pub fn periph_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64> {
    Ok(cpu_hz(slcr, ps_clk_hz)? / 2)
}

/// The UART reference clock: the selected PLL divided by `UART_CLK_CTRL.DIVISOR`.
pub fn uart_ref_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64> {
    let ctrl = slcr.read32(UART_CLK_CTRL)?;
    let pll = match (ctrl >> 4) & 0x3 {
        2 => Pll::Arm,
        3 => Pll::Ddr,
        _ => Pll::Io,
    };
    let div = u64::from((ctrl >> 8) & 0x3F).max(1);
    Ok(pll_hz(slcr, ps_clk_hz, pll)? / div)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    fn slcr(vals: &[(usize, u32)]) -> hardroxide_mmio::MockRegion {
        let be = MockBackend::new();
        for &(o, v) in vals {
            be.poke(0xF800_0000 + o as u64, v);
        }
        be.map(0xF800_0000, 0x1000).unwrap()
    }

    #[test]
    fn matches_the_vendor_configuration_measured_on_the_board() {
        // After the fixture's ps7_init on a PYNQ-Z2: ARM PLL x26, divisor 2 -> 650 MHz CPU,
        // 325 MHz private peripherals (the global timer ran at 325 MHz on the board).
        let r = slcr(&[(ARM_PLL_CTRL, 0x0001A000), (ARM_CLK_CTRL, 0x1F00_0200)]);
        assert_eq!(pll_hz(&r, 50_000_000, Pll::Arm).unwrap(), 1_300_000_000);
        assert_eq!(cpu_hz(&r, 50_000_000).unwrap(), 650_000_000);
        assert_eq!(periph_hz(&r, 50_000_000).unwrap(), 325_000_000);
    }

    #[test]
    fn a_bypassed_pll_passes_the_reference_through() {
        let r = slcr(&[(ARM_PLL_CTRL, 0x0001A010), (ARM_CLK_CTRL, 0x1F00_0100)]);
        assert_eq!(pll_hz(&r, 50_000_000, Pll::Arm).unwrap(), 50_000_000);
        assert_eq!(cpu_hz(&r, 50_000_000).unwrap(), 50_000_000);
    }

    #[test]
    fn uart_reference_follows_the_io_pll_and_divisor() {
        // JTAG-boot defaults seen on the board: IO PLL x26, UART divisor 13 -> 100 MHz.
        let r = slcr(&[(IO_PLL_CTRL, 0x0001A000), (UART_CLK_CTRL, 0x0000_0D03)]);
        assert_eq!(uart_ref_hz(&r, 50_000_000).unwrap(), 100_000_000);
        // Vendor design: IO PLL x20, divisor 10 -> 100 MHz as well.
        let r = slcr(&[(IO_PLL_CTRL, 0x00014000), (UART_CLK_CTRL, 0x0000_0A03)]);
        assert_eq!(uart_ref_hz(&r, 50_000_000).unwrap(), 100_000_000);
    }
}
