//! AXI IIC (`xilinx.com:ip:axi_iic`, PG090) as an I2C master, using the
//! dynamic controller logic: START/STOP are flagged in the TX FIFO words.
//! UNVERIFIED on hardware; offsets/bits are asserted against the HWH.

use embedded_hal::i2c::{self, ErrorKind, ErrorType, NoAcknowledgeSource, Operation};
use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

pub const SOFTR: usize = 0x40;
pub const CR: usize = 0x100;
pub const SR: usize = 0x104;
pub const TX_FIFO: usize = 0x108;
pub const RX_FIFO: usize = 0x10C;
pub const ISR: usize = 0x20;

const CR_EN: u32 = 1 << 0;
const CR_TX_FIFO_RST: u32 = 1 << 1;
const SR_BB: u32 = 1 << 2;
const SR_RX_FIFO_EMPTY: u32 = 1 << 6;
const SR_TX_FIFO_EMPTY: u32 = 1 << 7;
const TX_START: u32 = 1 << 8;
const TX_STOP: u32 = 1 << 9;
const ISR_ARB_LOST: u32 = 1 << 0;
const ISR_TX_ERR: u32 = 1 << 1; // NACK / transmit error
const SOFT_RESET_KEY: u32 = 0xA;

#[derive(Debug, thiserror::Error)]
pub enum IicError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("slave did not acknowledge")]
    Nack,
    #[error("arbitration lost")]
    ArbitrationLost,
    #[error("timed out waiting for the controller")]
    Timeout,
    #[error("read of {0} bytes exceeds the 255-byte controller limit per operation")]
    ReadTooLong(usize),
}

impl i2c::Error for IicError {
    fn kind(&self) -> ErrorKind {
        match self {
            IicError::Nack => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown),
            IicError::ArbitrationLost => ErrorKind::ArbitrationLoss,
            _ => ErrorKind::Other,
        }
    }
}

pub struct AxiIic<R: Mmio> {
    regs: R,
    timeout: Duration,
}

impl<R: Mmio> AxiIic<R> {
    pub fn new(regs: R) -> Self {
        Self {
            regs,
            timeout: Duration::from_millis(100),
        }
    }

    pub fn set_timeout(&mut self, t: Duration) {
        self.timeout = t;
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Soft-reset the core, flush the TX FIFO and enable it.
    pub fn init(&self) -> Result<(), IicError> {
        self.regs.write32(SOFTR, SOFT_RESET_KEY)?;
        self.regs.write32(CR, CR_TX_FIFO_RST)?;
        self.regs.write32(CR, CR_EN)?;
        self.regs.write32(ISR, 0xFF)?; // clear status (toggle-on-write)
        Ok(())
    }

    fn wait(&self, mut done: impl FnMut(&R) -> Result<bool, MmioError>) -> Result<(), IicError> {
        let t0 = Instant::now();
        loop {
            self.check_errors()?;
            if done(&self.regs)? {
                return Ok(());
            }
            if t0.elapsed() > self.timeout {
                return Err(IicError::Timeout);
            }
        }
    }

    fn check_errors(&self) -> Result<(), IicError> {
        let isr = self.regs.read32(ISR)?;
        if isr & ISR_TX_ERR != 0 {
            self.recover()?;
            return Err(IicError::Nack);
        }
        if isr & ISR_ARB_LOST != 0 {
            self.recover()?;
            return Err(IicError::ArbitrationLost);
        }
        Ok(())
    }

    fn recover(&self) -> Result<(), IicError> {
        self.regs.write32(ISR, 0xFF)?;
        self.init()
    }

    fn tx(&self, word: u32) -> Result<(), IicError> {
        self.regs.write32(TX_FIFO, word)?;
        Ok(())
    }

    /// Run a sequence of operations as one I2C transaction (repeated starts
    /// between direction changes, a single STOP at the end).
    pub fn transaction_ops(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), IicError> {
        let n = ops.len();
        let mut prev_read: Option<bool> = None;
        for (idx, op) in ops.iter_mut().enumerate() {
            let last = idx + 1 == n;
            let is_read = matches!(op, Operation::Read(_));
            let new_segment = prev_read != Some(is_read);
            prev_read = Some(is_read);
            match op {
                Operation::Write(data) => {
                    if new_segment {
                        let stop = if data.is_empty() && last { TX_STOP } else { 0 };
                        self.tx(TX_START | stop | ((addr as u32) << 1))?;
                    }
                    for (i, b) in data.iter().enumerate() {
                        let stop = if last && i + 1 == data.len() {
                            TX_STOP
                        } else {
                            0
                        };
                        self.tx(stop | *b as u32)?;
                    }
                }
                Operation::Read(buf) => {
                    if buf.len() > 255 {
                        return Err(IicError::ReadTooLong(buf.len()));
                    }
                    if buf.is_empty() {
                        continue;
                    }
                    if new_segment {
                        self.tx(TX_START | ((addr as u32) << 1) | 1)?;
                    }
                    let stop = if last { TX_STOP } else { 0 };
                    self.tx(stop | buf.len() as u32)?; // number of bytes to receive
                    for b in buf.iter_mut() {
                        self.wait(|r| Ok(r.read32(SR)? & SR_RX_FIFO_EMPTY == 0))?;
                        *b = self.regs.read32(RX_FIFO)? as u8;
                    }
                }
            }
        }
        self.wait(|r| Ok(r.read32(SR)? & SR_TX_FIFO_EMPTY != 0))?;
        self.wait(|r| Ok(r.read32(SR)? & SR_BB == 0))?;
        Ok(())
    }
}

impl<R: Mmio> ErrorType for AxiIic<R> {
    type Error = IicError;
}

impl<R: Mmio> i2c::I2c for AxiIic<R> {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), IicError> {
        self.transaction_ops(address, operations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::I2c;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// Minimal model of the core: logs TX FIFO words, serves queued RX bytes,
    /// reports idle bus and an empty TX FIFO. Can inject a NACK.
    #[derive(Default)]
    struct FakeIic {
        tx: RefCell<Vec<u32>>,
        rx: RefCell<VecDeque<u8>>,
        nack: RefCell<bool>,
    }

    impl Mmio for FakeIic {
        fn len(&self) -> usize {
            0x1000
        }
        fn read32(&self, o: usize) -> Result<u32, MmioError> {
            Ok(match o {
                SR => {
                    SR_TX_FIFO_EMPTY
                        | if self.rx.borrow().is_empty() {
                            SR_RX_FIFO_EMPTY
                        } else {
                            0
                        }
                }
                RX_FIFO => self.rx.borrow_mut().pop_front().unwrap_or(0) as u32,
                ISR if *self.nack.borrow() => ISR_TX_ERR,
                _ => 0,
            })
        }
        fn write32(&self, o: usize, v: u32) -> Result<(), MmioError> {
            if o == TX_FIFO {
                self.tx.borrow_mut().push(v);
            }
            if o == ISR {
                *self.nack.borrow_mut() = false; // cleared by recovery
            }
            Ok(())
        }
    }

    #[test]
    fn write_then_read_uses_repeated_start_and_single_stop() {
        let fake = FakeIic::default();
        fake.rx.borrow_mut().extend([0xAA, 0xBB]);
        let mut i2c = AxiIic::new(fake);
        let mut buf = [0u8; 2];
        i2c.write_read(0x50, &[0x10, 0x20], &mut buf).unwrap();
        assert_eq!(buf, [0xAA, 0xBB]);
        let tx = i2c.regs.tx.borrow().clone();
        assert_eq!(
            tx,
            [
                TX_START | (0x50 << 1),     // addr + W, START
                0x10,                       // data
                0x20,                       // last write byte, no STOP (read follows)
                TX_START | (0x50 << 1) | 1, // repeated START, addr + R
                TX_STOP | 2,                // receive 2 bytes then STOP
            ]
        );
    }

    #[test]
    fn plain_write_ends_with_stop() {
        let mut i2c = AxiIic::new(FakeIic::default());
        i2c.write(0x3C, &[1, 2, 3]).unwrap();
        assert_eq!(
            *i2c.regs.tx.borrow(),
            [TX_START | (0x3C << 1), 1, 2, TX_STOP | 3]
        );
    }

    #[test]
    fn consecutive_writes_merge_and_nack_is_reported() {
        let mut i2c = AxiIic::new(FakeIic::default());
        i2c.transaction(0x10, &mut [Operation::Write(&[1]), Operation::Write(&[2])])
            .unwrap();
        assert_eq!(
            *i2c.regs.tx.borrow(),
            [TX_START | (0x10 << 1), 1, TX_STOP | 2]
        );
        *i2c.regs.nack.borrow_mut() = true;
        let e = i2c.write(0x10, &[1]).unwrap_err();
        assert!(matches!(e, IicError::Nack));
        assert!(matches!(i2c::Error::kind(&e), ErrorKind::NoAcknowledge(_)));
    }

    #[test]
    fn registers_and_bits_match_the_hwh() {
        let h = |r, f| crate::hwh("pynq_z2_peripherals", "axi_iic_0", r, f);
        for (r, o) in [
            ("SOFTR", SOFTR),
            ("CR", CR),
            ("SR", SR),
            ("TX_FIFO", TX_FIFO),
            ("RX_FIFO", RX_FIFO),
            ("ISR", ISR),
        ] {
            assert_eq!(h(r, None).0, o, "{r}");
        }
        for (f, m) in [("EN", CR_EN), ("TX_FIFO_Reset", CR_TX_FIFO_RST)] {
            assert_eq!(1u32 << h("CR", Some(f)).1, m, "{f}");
        }
        for (f, m) in [
            ("BB", SR_BB),
            ("RX_FIFO_Empty", SR_RX_FIFO_EMPTY),
            ("TX_FIFO_Empty", SR_TX_FIFO_EMPTY),
        ] {
            assert_eq!(1u32 << h("SR", Some(f)).1, m, "{f}");
        }
        assert_eq!(1u32 << h("TX_FIFO", Some("Start")).1, TX_START);
        assert_eq!(1u32 << h("TX_FIFO", Some("Stop")).1, TX_STOP);
        assert_eq!(1u32 << h("ISR", Some("int0")).1, ISR_ARB_LOST);
        assert_eq!(1u32 << h("ISR", Some("int1")).1, ISR_TX_ERR);
    }
}
