//! AXI UART Lite (`xilinx.com:ip:axi_uartlite`, PG142): 8N1-style serial port
//! with 16-byte FIFOs. Baud rate and framing are fixed at build time.
//! UNVERIFIED on hardware; offsets/bits are asserted against the HWH.

use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

pub const RX_FIFO: usize = 0x00;
pub const TX_FIFO: usize = 0x04;
pub const STAT_REG: usize = 0x08;
pub const CTRL_REG: usize = 0x0C;

const STAT_RX_VALID: u32 = 1 << 0;
const STAT_TX_FULL: u32 = 1 << 3;
const STAT_TX_EMPTY: u32 = 1 << 2;
const STAT_OVERRUN: u32 = 1 << 5;
const STAT_FRAME: u32 = 1 << 6;
const STAT_PARITY: u32 = 1 << 7;
const CTRL_RST_TX: u32 = 1 << 0;
const CTRL_RST_RX: u32 = 1 << 1;
const CTRL_INTR_EN: u32 = 1 << 4;

#[derive(Debug, thiserror::Error)]
pub enum UartError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("timed out after {0:?}")]
    Timeout(Duration),
}

/// Receive errors latched by the core; reading the status register clears them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RxErrors {
    pub overrun: bool,
    pub frame: bool,
    pub parity: bool,
}

pub struct AxiUart<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiUart<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Reset both FIFOs.
    pub fn init(&self) -> Result<(), UartError> {
        self.regs.write32(CTRL_REG, CTRL_RST_TX | CTRL_RST_RX)?;
        Ok(())
    }

    /// Enable or disable the interrupt (fires on RX data and TX FIFO empty).
    pub fn set_interrupts(&self, on: bool) -> Result<(), UartError> {
        self.regs
            .write32(CTRL_REG, if on { CTRL_INTR_EN } else { 0 })?;
        Ok(())
    }

    /// Receive errors since the last status read (this read clears them).
    pub fn errors(&self) -> Result<RxErrors, UartError> {
        let s = self.regs.read32(STAT_REG)?;
        Ok(RxErrors {
            overrun: s & STAT_OVERRUN != 0,
            frame: s & STAT_FRAME != 0,
            parity: s & STAT_PARITY != 0,
        })
    }

    /// True when every queued byte has left the transmit FIFO.
    pub fn tx_empty(&self) -> Result<bool, UartError> {
        Ok(self.regs.read32(STAT_REG)? & STAT_TX_EMPTY != 0)
    }

    /// Queue one byte; `false` if the transmit FIFO is full.
    pub fn try_write_byte(&self, b: u8) -> Result<bool, UartError> {
        if self.regs.read32(STAT_REG)? & STAT_TX_FULL != 0 {
            return Ok(false);
        }
        self.regs.write32(TX_FIFO, b as u32)?;
        Ok(true)
    }

    /// Take one received byte, if any.
    pub fn try_read_byte(&self) -> Result<Option<u8>, UartError> {
        if self.regs.read32(STAT_REG)? & STAT_RX_VALID == 0 {
            return Ok(None);
        }
        Ok(Some(self.regs.read32(RX_FIFO)? as u8))
    }

    /// Send all of `data`, waiting for FIFO space for up to `timeout`.
    pub fn send(&self, data: &[u8], timeout: Duration) -> Result<(), UartError> {
        let t0 = Instant::now();
        for &b in data {
            while !self.try_write_byte(b)? {
                if t0.elapsed() > timeout {
                    return Err(UartError::Timeout(timeout));
                }
            }
        }
        Ok(())
    }

    /// Read up to `buf.len()` bytes, waiting at most `timeout` in total.
    /// Returns how many arrived (possibly 0).
    pub fn receive(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, UartError> {
        let t0 = Instant::now();
        let mut n = 0;
        while n < buf.len() {
            match self.try_read_byte()? {
                Some(b) => {
                    buf[n] = b;
                    n += 1;
                }
                None if t0.elapsed() > timeout => break,
                None => {}
            }
        }
        Ok(n)
    }
}

impl<R: Mmio> std::io::Write for AxiUart<R> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.send(buf, Duration::from_secs(1))
            .map_err(std::io::Error::other)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let t0 = Instant::now();
        while !self.tx_empty().map_err(std::io::Error::other)? {
            if t0.elapsed() > Duration::from_secs(1) {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// FIFO model: TX capacity 16, RX queue served byte by byte.
    #[derive(Default)]
    struct FakeUart {
        tx: RefCell<Vec<u8>>,
        rx: RefCell<VecDeque<u8>>,
        ctrl: RefCell<u32>,
    }

    impl Mmio for FakeUart {
        fn len(&self) -> usize {
            0x1000
        }
        fn read32(&self, o: usize) -> Result<u32, MmioError> {
            Ok(match o {
                STAT_REG => {
                    let mut s = 0;
                    if !self.rx.borrow().is_empty() {
                        s |= STAT_RX_VALID;
                    }
                    if self.tx.borrow().len() >= 16 {
                        s |= STAT_TX_FULL;
                    }
                    if self.tx.borrow().is_empty() {
                        s |= STAT_TX_EMPTY;
                    }
                    s
                }
                RX_FIFO => self.rx.borrow_mut().pop_front().unwrap_or(0) as u32,
                _ => 0,
            })
        }
        fn write32(&self, o: usize, v: u32) -> Result<(), MmioError> {
            match o {
                TX_FIFO => self.tx.borrow_mut().push(v as u8),
                CTRL_REG => {
                    *self.ctrl.borrow_mut() = v;
                    if v & CTRL_RST_TX != 0 {
                        self.tx.borrow_mut().clear();
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    #[test]
    fn transmit_stops_when_fifo_is_full() {
        let u = AxiUart::new(FakeUart::default());
        for b in 0..16 {
            assert!(u.try_write_byte(b).unwrap());
        }
        assert!(!u.try_write_byte(99).unwrap());
        assert!(matches!(
            u.send(b"x", Duration::from_millis(5)),
            Err(UartError::Timeout(_))
        ));
        u.init().unwrap();
        assert!(u.try_write_byte(1).unwrap());
    }

    #[test]
    fn receive_returns_what_arrived() {
        let u = AxiUart::new(FakeUart::default());
        u.regs().rx.borrow_mut().extend(*b"hi");
        let mut buf = [0u8; 4];
        assert_eq!(u.receive(&mut buf, Duration::from_millis(5)).unwrap(), 2);
        assert_eq!(&buf[..2], b"hi");
        assert_eq!(u.try_read_byte().unwrap(), None);
    }

    #[test]
    fn interrupt_enable_bit() {
        let u = AxiUart::new(FakeUart::default());
        u.set_interrupts(true).unwrap();
        assert_eq!(*u.regs().ctrl.borrow(), CTRL_INTR_EN);
    }

    #[test]
    fn registers_and_bits_match_the_hwh() {
        let h = |r, f| crate::hwh("pynq_z2_mem_uart", "axi_uartlite_0", r, f);
        for (r, o) in [
            ("RX_FIFO", RX_FIFO),
            ("TX_FIFO", TX_FIFO),
            ("STAT_REG", STAT_REG),
            ("CTRL_REG", CTRL_REG),
        ] {
            assert_eq!(h(r, None).0, o, "{r}");
        }
        for (f, m) in [
            ("RX_FIFO_Valid_Data", STAT_RX_VALID),
            ("TX_FIFO_Empty", STAT_TX_EMPTY),
            ("TX_FIFO_Full", STAT_TX_FULL),
            ("Overrun_Error", STAT_OVERRUN),
            ("Frame_Error", STAT_FRAME),
            ("Parity_Error", STAT_PARITY),
        ] {
            assert_eq!(1u32 << h("STAT_REG", Some(f)).1, m, "{f}");
        }
        for (f, m) in [
            ("RST_TXFIFO", CTRL_RST_TX),
            ("RST_RXFIFO", CTRL_RST_RX),
            ("Enable_Intr", CTRL_INTR_EN),
        ] {
            assert_eq!(1u32 << h("CTRL_REG", Some(f)).1, m, "{f}");
        }
    }
}
