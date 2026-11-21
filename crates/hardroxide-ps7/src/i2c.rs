//! The Zynq-7000's Cadence I2C controllers (`I2C0` and `I2C1`), master mode, polled, 7-bit
//! addresses. The register layout and transfer sequences follow the vendor driver's header and
//! polled master functions. On the PYNQ-Z2's base overlay `I2C1` is routed through the EMIO to the
//! audio codec's control pins.
//!
//! A transfer is at most one FIFO (16 bytes), which is all the codec and sensor devices this
//! crate is used with need; longer transfers are refused rather than split. `write_read` is two
//! transfers with a stop between (what the codec's driver in PYNQ does), not a repeated start.

use core::time::Duration;
use embedded_hal::i2c::{self, ErrorKind, ErrorType, NoAcknowledgeSource, Operation};
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{Mmio, MmioError};

const CR: usize = 0x00;
const SR: usize = 0x04;
const ADDR: usize = 0x08;
const DATA: usize = 0x0C;
const ISR: usize = 0x10;
const TRANS_SIZE: usize = 0x14;

const CR_RD_WR: u32 = 1 << 0;
const CR_MS: u32 = 1 << 1;
const CR_NEA: u32 = 1 << 2;
const CR_ACKEN: u32 = 1 << 3;
const CR_CLR_FIFO: u32 = 1 << 6;
const CR_DIV_MASK: u32 = 0x0000_FF00 | 0x0000_C000;

const SR_RXDV: u32 = 1 << 5;

const IXR_COMP: u32 = 1 << 0;
const IXR_NACK: u32 = 1 << 2;
const IXR_TO: u32 = 1 << 3;
const IXR_ARB_LOST: u32 = 1 << 9;
const IXR_ALL: u32 = 0x2FF;

/// Bytes in the transmit and receive FIFOs: the longest transfer.
pub const FIFO_DEPTH: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum I2cError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("no device acknowledged address {0:#x}")]
    Nack(u8),
    #[error("lost arbitration")]
    ArbitrationLost,
    #[error("the transfer did not complete in time")]
    Timeout,
    #[error("a transfer is at most {FIFO_DEPTH} bytes")]
    TooLong,
    #[error("the requested bus clock cannot be generated from this input clock")]
    BadClock,
}

impl i2c::Error for I2cError {
    fn kind(&self) -> ErrorKind {
        match self {
            I2cError::Nack(_) => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown),
            I2cError::ArbitrationLost => ErrorKind::ArbitrationLoss,
            _ => ErrorKind::Other,
        }
    }
}

/// `(div_a, div_b, achieved_hz)` for a bus clock near `scl_hz` from an input clock of `input_hz`:
/// `f = input / (22 * (div_a + 1) * (div_b + 1))`, searched the way the vendor driver does.
pub fn clock_divisors(input_hz: u32, scl_hz: u32) -> Option<(u32, u32, u32)> {
    if scl_hz == 0 {
        return None;
    }
    let mut want = scl_hz;
    let temp = input_hz / (22 * want);
    if temp == 0 {
        return None;
    }
    if want > 384_600 {
        want = 384_600;
    }
    if want <= 100_000 && want > 90_000 {
        want = 90_000;
    }
    let limit = if !input_hz.is_multiple_of(22 * want) {
        temp + 1
    } else {
        temp
    };
    let (mut best_err, mut best) = (want, (0u32, 0u32));
    for t in temp..=limit {
        let (mut last_err, mut calc) = (want, (0u32, 0u32));
        for b in 0..64u32 {
            let a = (t / (b + 1)).saturating_sub(1);
            if a > 3 {
                continue;
            }
            let actual = input_hz / (22 * (a + 1) * (b + 1));
            let err = actual.abs_diff(want);
            if last_err > err {
                calc = (a, b);
                last_err = err;
            }
        }
        if last_err < best_err {
            best_err = last_err;
            best = calc;
        }
    }
    Some((
        best.0,
        best.1,
        input_hz / (22 * (best.0 + 1) * (best.1 + 1)),
    ))
}

pub struct I2c<R: Mmio> {
    regs: R,
}

impl<R: Mmio> I2c<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Set the bus clock to about `scl_hz` given the controller's input clock
    /// ([`crate::clocks::cpu_1x_hz`]); returns the clock produced.
    pub fn set_clock(&self, input_hz: u32, scl_hz: u32) -> Result<u32, I2cError> {
        let (a, b, hz) = clock_divisors(input_hz, scl_hz).ok_or(I2cError::BadClock)?;
        let cr = self.regs.read32(CR)? & !CR_DIV_MASK;
        self.regs.write32(CR, cr | a << 14 | b << 8)?;
        Ok(hz)
    }

    fn control(&self, read: bool) -> Result<(), MmioError> {
        let divisors = self.regs.read32(CR)? & CR_DIV_MASK;
        let dir = if read { CR_RD_WR } else { 0 };
        self.regs
            .write32(CR, divisors | CR_MS | CR_NEA | CR_ACKEN | CR_CLR_FIFO | dir)
    }

    /// Set up a write of `data` to `addr` and start it (writing the address starts the transfer).
    fn begin_write(&self, addr: u8, data: &[u8]) -> Result<(), I2cError> {
        if data.len() > FIFO_DEPTH {
            return Err(I2cError::TooLong);
        }
        self.regs.write32(ISR, IXR_ALL)?;
        self.control(false)?;
        for b in data {
            self.regs.write32(DATA, u32::from(*b))?;
        }
        self.regs.write32(ADDR, u32::from(addr))?;
        Ok(())
    }

    fn begin_read(&self, addr: u8, len: usize) -> Result<(), I2cError> {
        if len > FIFO_DEPTH {
            return Err(I2cError::TooLong);
        }
        self.regs.write32(ISR, IXR_ALL)?;
        self.control(true)?;
        self.regs.write32(TRANS_SIZE, len as u32)?;
        self.regs.write32(ADDR, u32::from(addr))?;
        Ok(())
    }

    /// Wait for the transfer to finish or fail.
    fn wait_done(&self, addr: u8) -> Result<(), I2cError> {
        let t0 = Instant::now();
        loop {
            let isr = self.regs.read32(ISR)?;
            if isr & IXR_NACK != 0 {
                self.regs.write32(ISR, IXR_NACK)?;
                return Err(I2cError::Nack(addr));
            }
            if isr & IXR_ARB_LOST != 0 {
                self.regs.write32(ISR, IXR_ARB_LOST)?;
                return Err(I2cError::ArbitrationLost);
            }
            if isr & IXR_TO != 0 {
                self.regs.write32(ISR, IXR_TO)?;
                return Err(I2cError::Timeout);
            }
            if isr & IXR_COMP != 0 {
                self.regs.write32(ISR, IXR_COMP)?;
                return Ok(());
            }
            if t0.elapsed() > Duration::from_millis(50) {
                return Err(I2cError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    /// Write `data` (up to 16 bytes) to the device at 7-bit address `addr`.
    pub fn write(&self, addr: u8, data: &[u8]) -> Result<(), I2cError> {
        self.begin_write(addr, data)?;
        self.wait_done(addr)
    }

    /// Read `buf.len()` (up to 16) bytes from the device at `addr`.
    pub fn read(&self, addr: u8, buf: &mut [u8]) -> Result<(), I2cError> {
        self.begin_read(addr, buf.len())?;
        self.wait_done(addr)?;
        for b in buf.iter_mut() {
            if self.regs.read32(SR)? & SR_RXDV == 0 {
                return Err(I2cError::Timeout);
            }
            *b = self.regs.read32(DATA)? as u8;
        }
        Ok(())
    }

    /// A write followed by a read, as two transfers (a stop between).
    pub fn write_read(&self, addr: u8, out: &[u8], buf: &mut [u8]) -> Result<(), I2cError> {
        self.write(addr, out)?;
        self.read(addr, buf)
    }
}

impl<R: Mmio> ErrorType for I2c<R> {
    type Error = I2cError;
}

impl<R: Mmio> i2c::I2c for I2c<R> {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), I2cError> {
        for op in operations {
            match op {
                Operation::Write(d) => self.write(address, d)?,
                Operation::Read(b) => self.read(address, b)?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const BASE: u64 = 0xE000_5000;

    #[test]
    fn four_hundred_khz_from_the_cpu_1x_clock() {
        // 108.33 MHz / (22 * 1 * 12) is about 410 kHz; the search picks the closest it can reach.
        let (a, b, hz) = clock_divisors(108_333_333, 400_000).unwrap();
        assert!(a <= 3 && b < 64);
        assert!(
            hz.abs_diff(384_600) < 40_000 || hz.abs_diff(400_000) < 40_000,
            "{hz}"
        );
        assert_eq!(hz, 108_333_333 / (22 * (a + 1) * (b + 1)));
        assert!(clock_divisors(1_000, 400_000).is_none());
        assert!(clock_divisors(100_000_000, 0).is_none());
    }

    #[test]
    fn a_hundred_kilohertz_is_limited_to_ninety() {
        // The vendor driver lowers 100 kHz targets to 90 kHz: standard mode at its worst case.
        let (_, _, hz) = clock_divisors(108_333_333, 100_000).unwrap();
        assert!(hz <= 100_000, "{hz}");
    }

    #[test]
    fn a_write_loads_the_fifo_then_the_address_last() {
        let be = MockBackend::new();
        let i = I2c::new(be.map(BASE, 0x1000).unwrap());
        i.set_clock(108_333_333, 400_000).unwrap();
        let divs = be.peek(BASE + CR as u64) & CR_DIV_MASK;
        i.begin_write(0x3B, &[0x40, 0x00, 0x0E]).unwrap();
        // Master, 7-bit addressing, ACK enabled, write direction, divisors kept.
        assert_eq!(
            be.peek(BASE + CR as u64),
            divs | CR_MS | CR_NEA | CR_ACKEN | CR_CLR_FIFO
        );
        let writes: std::vec::Vec<(u64, u32)> = be
            .log()
            .iter()
            .filter_map(|a| match a {
                hardroxide_mmio::MockAccess::Write { addr, value } => Some((*addr - BASE, *value)),
                _ => None,
            })
            .collect();
        assert_eq!(*writes.last().unwrap(), (ADDR as u64, 0x3B));
        let data: std::vec::Vec<u32> = writes
            .iter()
            .filter(|(a, _)| *a == DATA as u64)
            .map(|(_, v)| *v)
            .collect();
        assert_eq!(data, [0x40, 0x00, 0x0E]);
    }

    #[test]
    fn a_read_sets_the_direction_and_length() {
        let be = MockBackend::new();
        let i = I2c::new(be.map(BASE, 0x1000).unwrap());
        i.begin_read(0x3B, 6).unwrap();
        assert_eq!(be.peek(BASE + CR as u64) & CR_RD_WR, CR_RD_WR);
        assert_eq!(be.peek(BASE + TRANS_SIZE as u64), 6);
        assert_eq!(be.peek(BASE + ADDR as u64), 0x3B);
    }

    #[test]
    fn completion_and_failures_are_reported() {
        let be = MockBackend::new();
        let i = I2c::new(be.map(BASE, 0x1000).unwrap());
        be.poke(BASE + ISR as u64, IXR_COMP);
        assert!(i.wait_done(0x3B).is_ok());
        be.poke(BASE + ISR as u64, IXR_NACK);
        assert!(matches!(i.wait_done(0x3B), Err(I2cError::Nack(0x3B))));
        be.poke(BASE + ISR as u64, IXR_ARB_LOST);
        assert!(matches!(i.wait_done(0x3B), Err(I2cError::ArbitrationLost)));
    }

    #[test]
    fn long_transfers_are_refused() {
        let be = MockBackend::new();
        let i = I2c::new(be.map(BASE, 0x1000).unwrap());
        assert!(matches!(i.write(0x3B, &[0; 17]), Err(I2cError::TooLong)));
    }
}
