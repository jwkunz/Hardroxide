//! The Cadence SPI controllers of the Zynq-7000 and Zynq UltraScale+ (`SPI0`, `SPI1`), master mode, polled, 8-bit words,
//! with the chip selects driven by software: a select stays asserted for the whole of a [`Spi::transfer`].
//!
//! The register layout follows the vendor driver's header. The controller's pins and clock must already be set up
//! (`ps7_init`/`psu_init`).

use core::time::Duration;
use embedded_hal::spi::{self, ErrorKind, ErrorType, Operation, SpiDevice};
use hardroxide_mmio::time::{self, Instant};
use hardroxide_mmio::{Mmio, MmioError};

const CR: usize = 0x00;
const ISR: usize = 0x04;
const IDR: usize = 0x0C;
const ER: usize = 0x14;
const TXD: usize = 0x1C;
const RXD: usize = 0x20;
const TX_THRESHOLD: usize = 0x28;
const RX_THRESHOLD: usize = 0x2C;

const CR_MASTER: u32 = 1 << 0;
const CR_CPOL: u32 = 1 << 1;
const CR_CPHA: u32 = 1 << 2;
const CR_PRESCALE_SHIFT: u32 = 3;
const CR_SELECT_SHIFT: u32 = 10;
const CR_MANUAL_SELECT: u32 = 1 << 14;
/// The select field's value with no chip selected.
const NO_SELECT: u32 = 0xF;
const ISR_RX_OVERFLOW: u32 = 1 << 0;
const ISR_MODE_FAULT: u32 = 1 << 1;
const ISR_RX_NOT_EMPTY: u32 = 1 << 4;

/// Bytes moved per round: well within the FIFOs.
const CHUNK: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum SpiError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("the controller has chip selects 0 to 2")]
    BadSelect,
    #[error("the bus clock cannot be made slow enough from this input clock")]
    BadClock,
    #[error("the transfer did not complete in time")]
    Timeout,
}

/// Clock polarity and phase, by their usual numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Clock idles low, data sampled on the first edge.
    Mode0,
    /// Clock idles low, sampled on the second edge.
    Mode1,
    /// Clock idles high, sampled on the first edge.
    Mode2,
    /// Clock idles high, sampled on the second edge.
    Mode3,
}

/// `(prescale field, bus Hz)` for the fastest bus clock not above `max_hz`: the controller divides its input by
/// `2 << field`, with the field 1 to 7 (4 to 256).
pub fn prescale(input_hz: u32, max_hz: u32) -> Option<(u32, u32)> {
    (1..=7u32)
        .map(|field| (field, input_hz / (2 << field)))
        .find(|&(_, hz)| hz <= max_hz)
}

pub struct Spi<R: Mmio> {
    regs: R,
}

impl<R: Mmio> Spi<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Set the controller up as master in `mode` at no more than `max_hz` from an input clock of `input_hz`, with
    /// nothing selected. Returns the bus clock.
    pub fn configure(&self, mode: Mode, input_hz: u32, max_hz: u32) -> Result<u32, SpiError> {
        let (field, hz) = prescale(input_hz, max_hz).ok_or(SpiError::BadClock)?;
        let polarity = match mode {
            Mode::Mode0 => 0,
            Mode::Mode1 => CR_CPHA,
            Mode::Mode2 => CR_CPOL,
            Mode::Mode3 => CR_CPOL | CR_CPHA,
        };
        self.regs.write32(ER, 0)?;
        self.regs.write32(IDR, 0x7F)?;
        self.regs.write32(ISR, ISR_RX_OVERFLOW | ISR_MODE_FAULT)?;
        self.regs.write32(TX_THRESHOLD, 1)?;
        self.regs.write32(RX_THRESHOLD, 1)?;
        self.regs.write32(
            CR,
            CR_MASTER
                | polarity
                | field << CR_PRESCALE_SHIFT
                | CR_MANUAL_SELECT
                | NO_SELECT << CR_SELECT_SHIFT,
        )?;
        // Whatever an earlier user left in the receive FIFO.
        for _ in 0..128 {
            if self.regs.read32(ISR)? & ISR_RX_NOT_EMPTY == 0 {
                break;
            }
            self.regs.read32(RXD)?;
        }
        self.regs.write32(ER, 1)?;
        Ok(hz)
    }

    fn select(&self, field: u32) -> Result<(), MmioError> {
        self.regs.modify32(CR, |v| {
            (v & !(NO_SELECT << CR_SELECT_SHIFT)) | field << CR_SELECT_SHIFT
        })
    }

    /// Send `out` to the device on chip select `cs` (0 to 2) while receiving as many bytes into `back`, which must be
    /// empty (receive nothing) or as long as `out`.
    pub fn transfer(&self, cs: u8, out: &[u8], back: &mut [u8]) -> Result<(), SpiError> {
        if cs > 2 || !(back.is_empty() || back.len() == out.len()) {
            return Err(SpiError::BadSelect);
        }
        self.select(NO_SELECT & !(1 << cs))?;
        let result = self.exchange(out, back);
        self.select(NO_SELECT)?;
        result
    }

    /// Send `out` and discard what comes back.
    pub fn write(&self, cs: u8, out: &[u8]) -> Result<(), SpiError> {
        self.transfer(cs, out, &mut [])
    }

    /// The device on chip select `cs` as an [`embedded_hal::spi::SpiDevice`], for chip drivers. Set the mode and
    /// clock with [`Spi::configure`] first.
    pub fn device(&self, cs: u8) -> Device<'_, R> {
        Device { spi: self, cs }
    }

    fn operation(&self, op: &mut Operation<'_, u8>) -> Result<(), SpiError> {
        match op {
            Operation::Write(out) => self.exchange(out, &mut []),
            Operation::Read(back) => back
                .chunks_mut(CHUNK)
                .try_for_each(|chunk| self.exchange(&[0; CHUNK][..chunk.len()], chunk)),
            Operation::Transfer(back, out) => {
                // The longer side sets the length: zeros are sent past the end of `out`, and bytes past the end of
                // `back` are discarded.
                let total = back.len().max(out.len());
                let mut done = 0;
                while done < total {
                    let n = CHUNK.min(total - done);
                    let (mut tx, mut rx) = ([0u8; CHUNK], [0u8; CHUNK]);
                    for (i, b) in tx[..n].iter_mut().enumerate() {
                        *b = out.get(done + i).copied().unwrap_or(0);
                    }
                    self.exchange(&tx[..n], &mut rx[..n])?;
                    for (i, &b) in rx[..n].iter().enumerate() {
                        if let Some(slot) = back.get_mut(done + i) {
                            *slot = b;
                        }
                    }
                    done += n;
                }
                Ok(())
            }
            Operation::TransferInPlace(words) => words.chunks_mut(CHUNK).try_for_each(|chunk| {
                let mut tx = [0u8; CHUNK];
                tx[..chunk.len()].copy_from_slice(chunk);
                self.exchange(&tx[..chunk.len()], chunk)
            }),
            Operation::DelayNs(ns) => {
                time::sleep(Duration::from_nanos(u64::from(*ns)));
                Ok(())
            }
        }
    }

    fn exchange(&self, out: &[u8], back: &mut [u8]) -> Result<(), SpiError> {
        for (n, chunk) in out.chunks(CHUNK).enumerate() {
            for &b in chunk {
                self.regs.write32(TXD, u32::from(b))?;
            }
            // A byte arrives for every byte sent; the last one marks the end of the chunk.
            for i in 0..chunk.len() {
                let t0 = Instant::now();
                while self.regs.read32(ISR)? & ISR_RX_NOT_EMPTY == 0 {
                    if t0.elapsed() > Duration::from_millis(50) {
                        return Err(SpiError::Timeout);
                    }
                    core::hint::spin_loop();
                }
                let b = self.regs.read32(RXD)? as u8;
                if let Some(slot) = back.get_mut(n * CHUNK + i) {
                    *slot = b;
                }
            }
        }
        Ok(())
    }
}

impl spi::Error for SpiError {
    fn kind(&self) -> ErrorKind {
        match self {
            SpiError::BadSelect => ErrorKind::ChipSelectFault,
            _ => ErrorKind::Other,
        }
    }
}

/// One chip select of a controller ([`Spi::device`]).
pub struct Device<'a, R: Mmio> {
    spi: &'a Spi<R>,
    cs: u8,
}

impl<R: Mmio> ErrorType for Device<'_, R> {
    type Error = SpiError;
}

impl<R: Mmio> SpiDevice for Device<'_, R> {
    /// All the operations happen with the chip selected throughout. Reading sends zeros.
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), SpiError> {
        if self.cs > 2 {
            return Err(SpiError::BadSelect);
        }
        self.spi.select(NO_SELECT & !(1 << self.cs))?;
        let result = operations
            .iter_mut()
            .try_for_each(|op| self.spi.operation(op));
        self.spi.select(NO_SELECT)?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockAccess, MockBackend, MockRegion};

    const BASE: u64 = 0xFF05_0000;

    fn spi() -> (MockBackend, Spi<MockRegion>) {
        let be = MockBackend::new();
        let s = Spi::new(be.map(BASE, 0x100).unwrap());
        (be, s)
    }

    fn writes_to(be: &MockBackend, offset: usize) -> std::vec::Vec<u32> {
        be.log()
            .iter()
            .filter_map(|a| match *a {
                MockAccess::Write { addr, value } if addr == BASE + offset as u64 => Some(value),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_prescaler_never_exceeds_the_limit() {
        // 200 MHz: /256 = 781 kHz is the slowest.
        assert_eq!(prescale(200_000_000, 1_000_000), Some((7, 781_250)));
        assert_eq!(prescale(200_000_000, 50_000_000), Some((1, 50_000_000)));
        assert_eq!(prescale(200_000_000, 500_000), None);
        assert_eq!(prescale(100_000_000, 500_000), Some((7, 390_625)));
    }

    #[test]
    fn configuration_sets_mode_clock_and_manual_select() {
        let (be, s) = spi();
        assert_eq!(
            s.configure(Mode::Mode3, 200_000_000, 1_000_000).unwrap(),
            781_250
        );
        let cr = be.peek(BASE + CR as u64);
        assert_eq!(cr, 1 | 2 | 4 | 7 << 3 | 1 << 14 | 0xF << 10);
        assert_eq!(be.peek(BASE + ER as u64), 1);
        s.configure(Mode::Mode0, 200_000_000, 1_000_000).unwrap();
        assert_eq!(be.peek(BASE + CR as u64) & 6, 0);
    }

    #[test]
    fn a_device_keeps_its_chip_selected_across_operations() {
        let (be, s) = spi();
        s.configure(Mode::Mode0, 200_000_000, 1_000_000).unwrap();
        be.poke(BASE + ISR as u64, ISR_RX_NOT_EMPTY);
        be.poke(BASE + RXD as u64, 0xA7);
        let before = writes_to(&be, CR).len();
        let (mut read, mut longer, mut place) = ([0u8; 40], [0u8; 3], [1u8, 2]);
        s.device(1)
            .transaction(&mut [
                Operation::Write(&[0x80]),
                Operation::Read(&mut read),
                Operation::Transfer(&mut longer, &[9]),
                Operation::TransferInPlace(&mut place),
            ])
            .unwrap();
        let selects: std::vec::Vec<u32> = writes_to(&be, CR)[before..]
            .iter()
            .map(|v| (v >> 10) & 0xF)
            .collect();
        assert_eq!(selects, [0xD, 0xF]);
        assert_eq!((read, longer, place), ([0xA7; 40], [0xA7; 3], [0xA7; 2]));
        let sent = writes_to(&be, TXD);
        assert_eq!(sent.len(), 1 + 40 + 3 + 2);
        assert_eq!(sent[0], 0x80);
        assert!(sent[1..41].iter().all(|&b| b == 0));
        assert_eq!(sent[41..], [9, 0, 0, 1, 2]);
        assert!(s.device(3).write(&[0]).is_err());
    }

    #[test]
    fn a_transfer_selects_one_chip_for_all_its_bytes_and_releases_it() {
        let (be, s) = spi();
        s.configure(Mode::Mode0, 200_000_000, 1_000_000).unwrap();
        // The mock's FIFO always has a byte, 0x5A.
        be.poke(BASE + ISR as u64, ISR_RX_NOT_EMPTY);
        be.poke(BASE + RXD as u64, 0x5A);
        let out: std::vec::Vec<u8> = (0..40).collect();
        for (cs, field) in [(0u8, 0xE), (1, 0xD), (2, 0xB)] {
            let before = writes_to(&be, CR).len();
            let mut back = std::vec![0u8; out.len()];
            s.transfer(cs, &out, &mut back).unwrap();
            assert!(back.iter().all(|&b| b == 0x5A));
            let selects: std::vec::Vec<u32> = writes_to(&be, CR)[before..]
                .iter()
                .map(|v| (v >> 10) & 0xF)
                .collect();
            assert_eq!(selects, [field, 0xF], "chip select {cs}");
        }
        assert_eq!(writes_to(&be, TXD).len(), 3 * 40);
        assert!(matches!(s.write(3, &out), Err(SpiError::BadSelect)));
        assert!(matches!(
            s.transfer(0, &out, &mut [0; 2]),
            Err(SpiError::BadSelect)
        ));
    }
}
