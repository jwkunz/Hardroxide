//! Zynq UltraScale+: the PL clocks (`pl_clk0..3`, from the clock registers of both power domains) and the widths of the
//! AXI ports between the processing system and the PL. With the `std` feature, also the Linux bitstream loader's checks
//! for this family and the port-width file of a generated crate.
//!
//! A boot loader sets the port widths for the design it starts with. A bitstream loaded later may use others, and has to
//! be given them: until then transfers through a port of the wrong width carry wrong data. The registers and values are
//! those PYNQ writes when it loads an overlay.

use crate::FpgaError;
use crate::slcr::best_divisors;
use hardroxide_mmio::{Mmio, MmioBackend};

#[cfg(feature = "std")]
mod host;
#[cfg(feature = "std")]
pub use host::*;

/// Clock and reset registers of the low-power domain.
pub const CRL_APB_BASE: u64 = 0xFF5E_0000;
pub const CRL_APB_SIZE: usize = 0x1000;
/// Clock and reset registers of the full-power domain.
pub const CRF_APB_BASE: u64 = 0xFD1A_0000;
pub const CRF_APB_SIZE: usize = 0x1000;

// CRL_APB.
const IOPLL: usize = 0x20;
const RPLL: usize = 0x30;
const PL_REF: usize = 0xC0;
// CRF_APB.
const DPLL: usize = 0x2C;
const DPLL_TO_LPD: usize = 0x4C;
/// The clock-enable bit of a PL reference register.
const CLOCK_ACTIVE: u32 = 1 << 24;

/// The rate of the PLL that PL clock source `select` names, and whether it runs: a PLL's output is the reference times
/// its feedback divider (plus a fraction when enabled), halved by `DIV2`.
fn source_hz(
    crl: &impl Mmio,
    crf: &impl Mmio,
    ps_ref_hz: u64,
    select: u32,
) -> Result<Option<u64>, FpgaError> {
    // 0 and 1: the I/O PLL; 2: the RPU's; 3: the DDR PLL's copy for the low-power domain. A PLL's fraction register is
    // 8 bytes after its control register.
    let (ctrl, fraction) = match select {
        2 => (crl.read32(RPLL)?, crl.read32(RPLL + 8)?),
        3 => (crf.read32(DPLL)?, crf.read32(DPLL + 8)?),
        _ => (crl.read32(IOPLL)?, crl.read32(IOPLL + 8)?),
    };
    let feedback = u64::from((ctrl >> 8) & 0x7F);
    // Bit 3: bypassed.
    if feedback == 0 || ctrl & (1 << 3) != 0 {
        return Ok(None);
    }
    let mut hz = ps_ref_hz * feedback;
    if fraction & (1 << 31) != 0 {
        hz += ps_ref_hz * u64::from(fraction & 0xFFFF) / 65_536;
    }
    if ctrl & (1 << 16) != 0 {
        hz /= 2;
    }
    if select == 3 {
        let divisor = u64::from((crf.read32(DPLL_TO_LPD)? >> 8) & 0x3F);
        hz /= divisor.max(1);
    }
    Ok(Some(hz))
}

fn register(index: u8) -> Result<usize, FpgaError> {
    if index > 3 {
        return Err(FpgaError::BadClockIndex(index));
    }
    Ok(PL_REF + 4 * usize::from(index))
}

/// Current frequency of `pl_clk<index>`; 0 if it is switched off or its PLL does not run. `ps_ref_hz` is the board's
/// reference (33.333 MHz on the RFSoC 4x2).
pub fn read_pl_clock(
    crl: &impl Mmio,
    crf: &impl Mmio,
    ps_ref_hz: u64,
    index: u8,
) -> Result<u64, FpgaError> {
    let ctrl = crl.read32(register(index)?)?;
    let (d0, d1) = (
        u64::from((ctrl >> 8) & 0x3F),
        u64::from((ctrl >> 16) & 0x3F),
    );
    if ctrl & CLOCK_ACTIVE == 0 || d0 == 0 || d1 == 0 {
        return Ok(0);
    }
    Ok(source_hz(crl, crf, ps_ref_hz, ctrl & 7)?.map_or(0, |hz| hz / (d0 * d1)))
}

/// Set `pl_clk<index>` to (approximately) `hz`, within 1 %, from the source it is set to, and switch it on. Returns the
/// frequency achieved.
pub fn set_pl_clock(
    crl: &impl Mmio,
    crf: &impl Mmio,
    ps_ref_hz: u64,
    index: u8,
    hz: u64,
) -> Result<u64, FpgaError> {
    let at = register(index)?;
    let ctrl = crl.read32(at)?;
    let Some(source) = source_hz(crl, crf, ps_ref_hz, ctrl & 7)? else {
        return Err(FpgaError::PllNotRunning(index));
    };
    let (d0, d1, got) = best_divisors(source, hz);
    if got.abs_diff(hz) * 100 > hz {
        return Err(FpgaError::ClockUnreachable {
            index,
            requested: hz,
            best: got,
            source_hz: source,
        });
    }
    crl.write32(
        at,
        (ctrl & !0x003F_3F00) | d0 << 8 | d1 << 16 | CLOCK_ACTIVE,
    )?;
    Ok(got)
}

/// An AXI port between the processing system and the PL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Port {
    /// A port the processor masters: 0 `HPM0_FPD`, 1 `HPM1_FPD`, 2 `HPM0_LPD`.
    Master(u8),
    /// A port the PL masters: `S_AXI_GP0..6` (`HPC0`, `HPC1`, `HP0..3`, `LPD`).
    Slave(u8),
}

impl Port {
    /// Whether the family has this port.
    pub fn exists(self) -> bool {
        match self {
            Port::Master(n) => n < 3,
            Port::Slave(n) => n < 7,
        }
    }
}

/// A port's data width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Bits32,
    Bits64,
    Bits128,
}

impl Width {
    pub fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            32 => Some(Width::Bits32),
            64 => Some(Width::Bits64),
            128 => Some(Width::Bits128),
            _ => None,
        }
    }

    pub fn bits(self) -> u32 {
        match self {
            Width::Bits32 => 32,
            Width::Bits64 => 64,
            Width::Bits128 => 128,
        }
    }
}

/// The registers that hold a port's width, as `(register address, bit position, value)`: two bits each. The two kinds of
/// port encode the widths in opposite order. Empty for a port that does not exist.
fn fields(port: Port, width: Width) -> [Option<(u64, u32, u32)>; 2] {
    if !port.exists() {
        return [None, None];
    }
    match port {
        Port::Master(n) => {
            let value = match width {
                Width::Bits32 => 0,
                Width::Bits64 => 1,
                Width::Bits128 => 2,
            };
            // FPD_SLCR for the two full-power ports, LPD_SLCR for the low-power one.
            let field = match n {
                0 => (0xFD61_5000, 8),
                1 => (0xFD61_5000, 10),
                _ => (0xFF41_9000, 8),
            };
            [Some((field.0, field.1, value)), None]
        }
        Port::Slave(n) => {
            let value = match width {
                Width::Bits32 => 2,
                Width::Bits64 => 1,
                Width::Bits128 => 0,
            };
            // The AXI FIFO interfaces: one block each, with a register for reads and one for writes.
            let base = if n < 6 {
                0xFD36_0000 + 0x1_0000 * u64::from(n)
            } else {
                0xFF9B_0000
            };
            [Some((base, 0, value)), Some((base + 0x14, 0, value))]
        }
    }
}

/// Give `port` the data width `width`, mapping its registers through `backend`. A port that does not exist is left
/// alone ([`Port::exists`]).
pub fn set_port_width(
    backend: &impl MmioBackend,
    port: Port,
    width: Width,
) -> Result<(), FpgaError> {
    for (address, shift, value) in fields(port, width).into_iter().flatten() {
        let regs = backend.map(address & !0xFFF, 0x1000)?;
        regs.modify32((address & 0xFFF) as usize, |v| {
            (v & !(3 << shift)) | value << shift
        })?;
    }
    Ok(())
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MockBackend, MockRegion};

    const REF: u64 = 33_333_333;

    /// As the RFSoC 4x2's `psu_init` leaves the registers.
    fn board() -> (MockBackend, MockRegion, MockRegion) {
        let be = MockBackend::new();
        be.poke(CRL_APB_BASE + 0x20, 0x0001_5A00); // I/O PLL: 90 x, halved: 1.5 GHz
        be.poke(CRL_APB_BASE + 0x30, 0x0001_3F00); // RPU PLL: 63 x, halved
        be.poke(CRF_APB_BASE + 0x2C, 0x0001_4800);
        be.poke(CRF_APB_BASE + 0x4C, 0x0000_0300);
        be.poke(CRL_APB_BASE + 0xC0, 0x0101_0F00); // pl_clk0: I/O PLL / 15
        be.poke(CRL_APB_BASE + 0xC4, 0x0101_0500); // pl_clk1: / 5
        let crl = be.map(CRL_APB_BASE, CRL_APB_SIZE).unwrap();
        let crf = be.map(CRF_APB_BASE, CRF_APB_SIZE).unwrap();
        (be, crl, crf)
    }

    fn near(got: u64, want: u64) {
        assert!(got.abs_diff(want) <= want / 100_000, "{got} not {want}");
    }

    #[test]
    fn reads_the_clocks_the_board_runs() {
        let (_be, crl, crf) = board();
        near(read_pl_clock(&crl, &crf, REF, 0).unwrap(), 99_999_999);
        near(read_pl_clock(&crl, &crf, REF, 1).unwrap(), 299_999_999);
        // Switched off.
        assert_eq!(read_pl_clock(&crl, &crf, REF, 2).unwrap(), 0);
        assert!(matches!(
            read_pl_clock(&crl, &crf, REF, 4),
            Err(FpgaError::BadClockIndex(4))
        ));
    }

    #[test]
    fn sets_the_dividers_and_switches_the_clock_on() {
        let (be, crl, crf) = board();
        near(
            set_pl_clock(&crl, &crf, REF, 0, 250_000_000).unwrap(),
            249_999_997,
        );
        assert_eq!(be.peek(CRL_APB_BASE + 0xC0), 0x0101_0600);
        // An off clock from the DDR PLL: 33.33 x 72 / 2 / 3 = 400 MHz, over 4.
        be.poke(CRL_APB_BASE + 0xC8, 0x0000_0003);
        near(
            set_pl_clock(&crl, &crf, REF, 2, 100_000_000).unwrap(),
            99_999_999,
        );
        near(read_pl_clock(&crl, &crf, REF, 2).unwrap(), 99_999_999);
        assert!(matches!(
            set_pl_clock(&crl, &crf, REF, 0, 7),
            Err(FpgaError::ClockUnreachable { .. })
        ));
        be.poke(CRL_APB_BASE + 0x20, 0x0000_0008); // bypassed
        assert!(matches!(
            set_pl_clock(&crl, &crf, REF, 0, 100_000_000),
            Err(FpgaError::PllNotRunning(0))
        ));
    }

    #[test]
    fn port_widths_go_where_pynq_writes_them() {
        let be = MockBackend::new();
        be.poke(0xFD61_5000, 0x0000_0A5A);
        set_port_width(&be, Port::Master(0), Width::Bits32).unwrap();
        set_port_width(&be, Port::Master(1), Width::Bits64).unwrap();
        // Bits 9:8 are port 0, 11:10 port 1; the rest is kept.
        assert_eq!(be.peek(0xFD61_5000), 0x0000_045A);
        set_port_width(&be, Port::Master(2), Width::Bits128).unwrap();
        assert_eq!(be.peek(0xFF41_9000), 2 << 8);
        // A port the PL masters: reads and writes, 128 bits encoded as 0 and 32 as 2.
        be.poke(0xFD38_0000, 0x3);
        set_port_width(&be, Port::Slave(2), Width::Bits128).unwrap();
        assert_eq!(be.peek(0xFD38_0000), 0);
        set_port_width(&be, Port::Slave(6), Width::Bits32).unwrap();
        assert_eq!((be.peek(0xFF9B_0000), be.peek(0xFF9B_0014)), (2, 2));
        let writes = be.log().len();
        set_port_width(&be, Port::Slave(7), Width::Bits32).unwrap();
        set_port_width(&be, Port::Master(3), Width::Bits32).unwrap();
        assert_eq!(be.log().len(), writes);
    }
}
