//! The Cadence UART of the Zynq-7000 PS (`UART0` at `0xE000_0000`, `UART1` at `0xE000_1000`).
//!
//! Polled transmit and receive, optional receive interrupts, and `core::fmt::Write` for
//! `write!`. The pins (MIO) and the UART clock must already be set up by `ps7_init`/the
//! FSBL; on the PYNQ-Z2 the USB-UART is UART0 on MIO 14 (RX) and 15 (TX).

use core::fmt;
use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

const CR: usize = 0x00;
const MR: usize = 0x04;
const IER: usize = 0x08;
const IDR: usize = 0x0C;
const ISR: usize = 0x14;
const BAUDGEN: usize = 0x18;
const RXWM: usize = 0x20;
const SR: usize = 0x2C;
const FIFO: usize = 0x30;
const BAUDDIV: usize = 0x34;

const CR_RXRST: u32 = 1 << 0;
const CR_TXRST: u32 = 1 << 1;
const CR_RX_EN: u32 = 1 << 2;
const CR_RX_DIS: u32 = 1 << 3;
const CR_TX_EN: u32 = 1 << 4;
const CR_TX_DIS: u32 = 1 << 5;
const MR_8N1: u32 = 0x20; // 8 data bits, no parity, 1 stop bit, normal mode
const SR_RXEMPTY: u32 = 1 << 1;
const SR_TXEMPTY: u32 = 1 << 3;
const SR_TXFULL: u32 = 1 << 4;
/// Interrupt `RTRIG`: the receive FIFO reached its trigger level.
pub const IXR_RX_TRIGGER: u32 = 1 << 0;
/// Interrupt: receive timeout.
pub const IXR_RX_TIMEOUT: u32 = 1 << 8;

/// Baud-rate generator settings: `baud = ref_clk / (BAUDGEN * (BAUDDIV + 1))`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaudSetting {
    pub baudgen: u32,
    pub bauddiv: u32,
}

impl BaudSetting {
    /// The baud rate these settings produce from `ref_clk_hz`.
    pub fn actual(&self, ref_clk_hz: u32) -> u32 {
        ref_clk_hz / (self.baudgen * (self.bauddiv + 1))
    }
}

/// Choose `BAUDGEN`/`BAUDDIV` for `baud` from a `ref_clk_hz` reference: the smallest
/// error over `BAUDDIV` 4..=255, the smallest `BAUDDIV` on a tie. `None` if the rate is
/// out of range for this clock.
pub fn baud_setting(ref_clk_hz: u32, baud: u32) -> Option<BaudSetting> {
    if baud == 0 {
        return None;
    }
    let mut best: Option<(u32, BaudSetting)> = None;
    for bauddiv in 4..=255u32 {
        let denom = baud as u64 * (bauddiv as u64 + 1);
        let gen_div = ((ref_clk_hz as u64 + denom / 2) / denom) as u32; // rounded
        if !(2..=65535).contains(&gen_div) {
            continue;
        }
        let s = BaudSetting {
            baudgen: gen_div,
            bauddiv,
        };
        let err = s.actual(ref_clk_hz).abs_diff(baud);
        if best.is_none_or(|(e, _)| err < e) {
            best = Some((err, s));
        }
    }
    best.map(|(_, s)| s)
}

/// A UART controller.
pub struct Uart<R: Mmio> {
    regs: R,
}

impl<R: Mmio> Uart<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Reset the controller and enable transmit and receive at `baud` (8N1). `ref_clk_hz`
    /// is the UART reference clock (see [`crate::clocks::uart_ref_hz`]). Returns the baud
    /// rate actually produced, or `None` (and changes nothing) if `baud` is out of range.
    pub fn init(&self, ref_clk_hz: u32, baud: u32) -> Result<Option<u32>> {
        let Some(s) = baud_setting(ref_clk_hz, baud) else {
            return Ok(None);
        };
        self.regs.write32(CR, CR_TX_DIS | CR_RX_DIS)?;
        self.regs.write32(CR, CR_RXRST | CR_TXRST)?;
        self.regs.write32(MR, MR_8N1)?;
        self.regs.write32(BAUDGEN, s.baudgen)?;
        self.regs.write32(BAUDDIV, s.bauddiv)?;
        self.regs.write32(CR, CR_TX_EN | CR_RX_EN)?;
        Ok(Some(s.actual(ref_clk_hz)))
    }

    /// Queue one byte if the transmit FIFO has room.
    pub fn try_write_byte(&self, b: u8) -> Result<bool> {
        if self.regs.read32(SR)? & SR_TXFULL != 0 {
            return Ok(false);
        }
        self.regs.write32(FIFO, b as u32)?;
        Ok(true)
    }

    /// Queue one byte, waiting for room (bounded: gives up after a very long spin and
    /// reports `false` rather than hang a program whose UART is not clocked).
    pub fn write_byte(&self, b: u8) -> Result<bool> {
        for _ in 0..50_000_000u32 {
            if self.try_write_byte(b)? {
                return Ok(true);
            }
            core::hint::spin_loop();
        }
        Ok(false)
    }

    /// Wait until everything queued has left the shift register's FIFO.
    pub fn flush(&self) -> Result<()> {
        for _ in 0..50_000_000u32 {
            if self.regs.read32(SR)? & SR_TXEMPTY != 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Ok(())
    }

    /// The next received byte, if any.
    pub fn try_read_byte(&self) -> Result<Option<u8>> {
        if self.regs.read32(SR)? & SR_RXEMPTY != 0 {
            return Ok(None);
        }
        Ok(Some(self.regs.read32(FIFO)? as u8))
    }

    /// Interrupt when the receive FIFO holds at least `trigger` bytes (1..=63), and on
    /// receive timeout. `trigger` of 0 disables receive interrupts.
    pub fn set_rx_interrupt(&self, trigger: u32) -> Result<()> {
        if trigger == 0 {
            return self.regs.write32(IDR, IXR_RX_TRIGGER | IXR_RX_TIMEOUT);
        }
        self.regs.write32(RXWM, trigger.min(63))?;
        self.regs.write32(IER, IXR_RX_TRIGGER | IXR_RX_TIMEOUT)
    }

    /// The interrupt status; write the bits back with [`clear_interrupts`](Self::clear_interrupts).
    pub fn interrupt_status(&self) -> Result<u32> {
        self.regs.read32(ISR)
    }

    pub fn clear_interrupts(&self, mask: u32) -> Result<()> {
        self.regs.write32(ISR, mask)
    }
}

impl<R: Mmio> fmt::Write for Uart<R> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' && !matches!(self.write_byte(b'\r'), Ok(true)) {
                return Err(fmt::Error);
            }
            if !matches!(self.write_byte(b), Ok(true)) {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn baud_settings_hit_the_standard_rates_closely() {
        // The values that produced readable output on the board: 100 MHz reference.
        let s = baud_setting(100_000_000, 115_200).unwrap();
        assert!(s.actual(100_000_000).abs_diff(115_200) <= 600, "{s:?}");
        for (clk, baud) in [
            (100_000_000u32, 9_600u32),
            (100_000_000, 460_800),
            (50_000_000, 115_200),
            (100_000_000, 921_600),
        ] {
            let s = baud_setting(clk, baud).unwrap_or_else(|| panic!("{clk}/{baud}"));
            let err = s.actual(clk).abs_diff(baud) as f64 / baud as f64;
            assert!(err < 0.01, "{clk}/{baud}: {s:?} err {err}");
            assert!((4..=255).contains(&s.bauddiv) && s.baudgen >= 2);
        }
        assert_eq!(baud_setting(100_000_000, 0), None);
        assert_eq!(baud_setting(1_000, 115_200), None);
    }

    #[test]
    fn init_programs_the_controller() {
        let be = MockBackend::new();
        let u = Uart::new(be.map(0xE000_0000, 0x1000).unwrap());
        let got = u.init(100_000_000, 115_200).unwrap().unwrap();
        assert!(got.abs_diff(115_200) <= 600);
        let s = baud_setting(100_000_000, 115_200).unwrap();
        assert_eq!(be.peek(0xE000_0000 + BAUDGEN as u64), s.baudgen);
        assert_eq!(be.peek(0xE000_0000 + BAUDDIV as u64), s.bauddiv);
        assert_eq!(be.peek(0xE000_0000 + MR as u64), MR_8N1);
        assert_eq!(be.peek(0xE000_0000), CR_TX_EN | CR_RX_EN);
        // An impossible rate changes nothing.
        let be2 = MockBackend::new();
        let u2 = Uart::new(be2.map(0xE000_0000, 0x1000).unwrap());
        assert_eq!(u2.init(1_000, 115_200).unwrap(), None);
        assert!(be2.log().is_empty());
    }

    #[test]
    fn writes_translate_newlines_and_respect_a_full_fifo() {
        let be = MockBackend::new();
        let mut u = Uart::new(be.map(0xE000_0000, 0x1000).unwrap());
        write!(u, "a\nb").unwrap();
        let sent: Vec<u32> = be
            .log()
            .into_iter()
            .filter_map(|a| match a {
                hardroxide_mmio::MockAccess::Write {
                    addr: 0xE000_0030,
                    value,
                } => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(sent, [b'a' as u32, b'\r' as u32, b'\n' as u32, b'b' as u32]);
        // TX FIFO full: a non-blocking write is refused and writes nothing.
        be.poke(0xE000_002C, SR_TXFULL);
        assert!(!u.try_write_byte(b'x').unwrap());
    }

    #[test]
    fn receive_reads_only_when_the_fifo_is_not_empty() {
        let be = MockBackend::new();
        let u = Uart::new(be.map(0xE000_0000, 0x1000).unwrap());
        be.poke(0xE000_002C, SR_RXEMPTY);
        assert_eq!(u.try_read_byte().unwrap(), None);
        be.poke(0xE000_002C, 0);
        be.poke(0xE000_0030, 0x41);
        assert_eq!(u.try_read_byte().unwrap(), Some(0x41));
    }
}
