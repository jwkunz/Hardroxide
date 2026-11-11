//! AXI Quad SPI (`xilinx.com:ip:axi_quad_spi`, PG153) in **standard SPI
//! mode** as a master with manual slave select. UNVERIFIED on hardware;
//! offsets/bits are asserted against the HWH.
//!
//! [`AxiSpi`] implements `embedded_hal::spi::SpiBus` (chip select driven by
//! [`AxiSpi::set_chip_select`]); [`AxiSpi::device`] returns a
//! `SpiDevice` that asserts a slave line for each transaction.

use embedded_hal::spi::{
    self, ErrorKind, ErrorType, Mode, Operation, Phase, Polarity, SpiBus, SpiDevice,
};
use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

pub const SRR: usize = 0x40;
pub const SPICR: usize = 0x60;
pub const SPISR: usize = 0x64;
pub const SPIDTR: usize = 0x68;
pub const SPIDRR: usize = 0x6C;
pub const SPISSR: usize = 0x70;

const SRR_RESET: u32 = 0x0A;
const CR_LOOP: u32 = 1 << 0;
const CR_SPE: u32 = 1 << 1;
const CR_MASTER: u32 = 1 << 2;
const CR_CPOL: u32 = 1 << 3;
const CR_CPHA: u32 = 1 << 4;
const CR_TX_RST: u32 = 1 << 5;
const CR_RX_RST: u32 = 1 << 6;
const CR_MANUAL_SS: u32 = 1 << 7;
const CR_INHIBIT: u32 = 1 << 8;
const CR_LSB_FIRST: u32 = 1 << 9;
const SR_RX_EMPTY: u32 = 1 << 0;

#[derive(Debug, thiserror::Error)]
pub enum SpiError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("timed out waiting for the SPI core")]
    Timeout,
    #[error("chip select {0} out of range")]
    BadChipSelect(usize),
}

impl spi::Error for SpiError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

pub struct AxiSpi<R: Mmio> {
    regs: R,
    fifo_depth: usize,
    num_ss: usize,
    cs: Option<usize>,
    cr: u32,
    timeout: Duration,
}

impl<R: Mmio> AxiSpi<R> {
    /// `fifo_depth`: HWH `C_FIFO_DEPTH` (16 or 256; 1 if the IP has no
    /// FIFOs). `num_ss`: HWH `C_NUM_SS_BITS`.
    pub fn new(regs: R, fifo_depth: usize, num_ss: usize) -> Self {
        Self {
            regs,
            fifo_depth: fifo_depth.max(1),
            num_ss: num_ss.clamp(1, 32),
            cs: None,
            cr: 0,
            timeout: Duration::from_millis(100),
        }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Reset the core and configure master mode, manual SS, and `mode`.
    /// The SCK rate is fixed by the IP (`C_SCK_RATIO`); it is not set here.
    pub fn init(&mut self, mode: Mode, lsb_first: bool, loopback: bool) -> Result<(), SpiError> {
        self.regs.write32(SRR, SRR_RESET)?;
        let mut cr = CR_MASTER | CR_MANUAL_SS | CR_INHIBIT | CR_SPE;
        if mode.polarity == Polarity::IdleHigh {
            cr |= CR_CPOL;
        }
        if mode.phase == Phase::CaptureOnSecondTransition {
            cr |= CR_CPHA;
        }
        if lsb_first {
            cr |= CR_LSB_FIRST;
        }
        if loopback {
            cr |= CR_LOOP;
        }
        self.regs.write32(SPICR, cr | CR_TX_RST | CR_RX_RST)?;
        self.regs.write32(SPICR, cr)?;
        self.cr = cr;
        self.regs.write32(SPISSR, !0)?; // all slaves deselected (active low)
        Ok(())
    }

    /// Select which slave line [`SpiBus`] transfers assert (`None` = none).
    pub fn set_chip_select(&mut self, cs: Option<usize>) -> Result<(), SpiError> {
        if let Some(c) = cs
            && c >= self.num_ss
        {
            return Err(SpiError::BadChipSelect(c));
        }
        self.cs = cs;
        Ok(())
    }

    fn ssr_value(cs: Option<usize>) -> u32 {
        match cs {
            Some(c) => !(1u32 << c),
            None => !0,
        }
    }

    /// Clock out `out` while capturing into `inp` (same length), in FIFO-
    /// sized chunks with the master inhibit toggled around each chunk.
    fn exchange(&mut self, out: &[u8], inp: &mut [u8]) -> Result<(), SpiError> {
        debug_assert_eq!(out.len(), inp.len());
        self.regs.write32(SPISSR, Self::ssr_value(self.cs))?;
        let mut off = 0;
        while off < out.len() {
            let n = self.fifo_depth.min(out.len() - off);
            for &b in &out[off..off + n] {
                self.regs.write32(SPIDTR, b as u32)?;
            }
            self.regs.write32(SPICR, self.cr & !CR_INHIBIT)?; // start shifting
            let t0 = Instant::now();
            let mut got = 0;
            while got < n {
                if self.regs.read32(SPISR)? & SR_RX_EMPTY == 0 {
                    inp[off + got] = self.regs.read32(SPIDRR)? as u8;
                    got += 1;
                } else if t0.elapsed() > self.timeout {
                    self.regs.write32(SPICR, self.cr)?;
                    self.regs.write32(SPISSR, !0)?;
                    return Err(SpiError::Timeout);
                }
            }
            self.regs.write32(SPICR, self.cr)?; // inhibit again
            off += n;
        }
        self.regs.write32(SPISSR, !0)?;
        Ok(())
    }

    /// A `SpiDevice` for slave line `cs`: asserted for each transaction.
    pub fn device(&mut self, cs: usize) -> Result<SpiSlave<'_, R>, SpiError> {
        if cs >= self.num_ss {
            return Err(SpiError::BadChipSelect(cs));
        }
        Ok(SpiSlave { bus: self, cs })
    }
}

impl<R: Mmio> ErrorType for AxiSpi<R> {
    type Error = SpiError;
}

impl<R: Mmio> SpiBus<u8> for AxiSpi<R> {
    fn read(&mut self, words: &mut [u8]) -> Result<(), SpiError> {
        let out = vec![0u8; words.len()];
        self.exchange(&out, words)
    }
    fn write(&mut self, words: &[u8]) -> Result<(), SpiError> {
        let mut sink = vec![0u8; words.len()];
        self.exchange(words, &mut sink)
    }
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), SpiError> {
        let n = read.len().max(write.len());
        let mut out = write.to_vec();
        out.resize(n, 0);
        let mut inp = vec![0u8; n];
        self.exchange(&out, &mut inp)?;
        let r = read.len();
        read.copy_from_slice(&inp[..r]);
        Ok(())
    }
    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), SpiError> {
        let out = words.to_vec();
        self.exchange(&out, words)
    }
    fn flush(&mut self) -> Result<(), SpiError> {
        Ok(()) // every call completes before returning
    }
}

pub struct SpiSlave<'a, R: Mmio> {
    bus: &'a mut AxiSpi<R>,
    cs: usize,
}

impl<R: Mmio> ErrorType for SpiSlave<'_, R> {
    type Error = SpiError;
}

impl<R: Mmio> SpiDevice<u8> for SpiSlave<'_, R> {
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), SpiError> {
        let saved = self.bus.cs;
        self.bus.cs = Some(self.cs);
        let mut result = Ok(());
        for op in operations.iter_mut() {
            result = match op {
                Operation::Read(b) => SpiBus::read(self.bus, b),
                Operation::Write(b) => SpiBus::write(self.bus, b),
                Operation::Transfer(r, w) => SpiBus::transfer(self.bus, r, w),
                Operation::TransferInPlace(b) => SpiBus::transfer_in_place(self.bus, b),
                Operation::DelayNs(ns) => {
                    std::thread::sleep(Duration::from_nanos(*ns as u64));
                    Ok(())
                }
            };
            if result.is_err() {
                break;
            }
        }
        // NB: `exchange` deasserts after each op; a multi-op transaction
        // therefore toggles CS between ops (see docs/08, G12).
        self.bus.cs = saved;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::spi::MODE_3;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// Loopback core model: every byte written to the TX FIFO appears in the
    /// RX FIFO (XOR 0xFF so tests notice swapped data). Logs SSR writes.
    #[derive(Default)]
    struct FakeSpi {
        rx: RefCell<VecDeque<u8>>,
        ssr: RefCell<Vec<u32>>,
        cr: RefCell<Vec<u32>>,
        tx_log: RefCell<Vec<u8>>,
    }

    impl Mmio for FakeSpi {
        fn len(&self) -> usize {
            0x1000
        }
        fn read32(&self, o: usize) -> Result<u32, MmioError> {
            Ok(match o {
                SPISR => {
                    if self.rx.borrow().is_empty() {
                        SR_RX_EMPTY
                    } else {
                        0
                    }
                }
                SPIDRR => self.rx.borrow_mut().pop_front().unwrap_or(0) as u32,
                _ => 0,
            })
        }
        fn write32(&self, o: usize, v: u32) -> Result<(), MmioError> {
            match o {
                SPIDTR => {
                    self.tx_log.borrow_mut().push(v as u8);
                    self.rx.borrow_mut().push_back(!(v as u8));
                }
                SPISSR => self.ssr.borrow_mut().push(v),
                SPICR => self.cr.borrow_mut().push(v),
                _ => {}
            }
            Ok(())
        }
    }

    #[test]
    fn init_encodes_mode_and_master() {
        let mut s = AxiSpi::new(FakeSpi::default(), 16, 1);
        s.init(MODE_3, false, false).unwrap();
        let last = *s.regs.cr.borrow().last().unwrap();
        assert_eq!(
            last,
            CR_MASTER | CR_MANUAL_SS | CR_INHIBIT | CR_SPE | CR_CPOL | CR_CPHA
        );
        assert_eq!(*s.regs.ssr.borrow(), [!0]);
    }

    #[test]
    fn transfer_chunks_through_small_fifo() {
        let mut s = AxiSpi::new(FakeSpi::default(), 4, 2);
        s.init(embedded_hal::spi::MODE_0, false, false).unwrap();
        s.set_chip_select(Some(1)).unwrap();
        let mut buf = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        s.transfer_in_place(&mut buf).unwrap();
        assert_eq!(buf, [!1u8, !2, !3, !4, !5, !6, !7, !8, !9, !10]);
        assert_eq!(*s.regs.tx_log.borrow(), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        // SS1 asserted (bit 1 low) during, all deasserted after.
        assert_eq!(s.regs.ssr.borrow()[1], !(1u32 << 1));
        assert_eq!(*s.regs.ssr.borrow().last().unwrap(), !0);
        // 10 bytes / 4 per chunk = 3 start pulses (inhibit released 3 times).
        let starts = s
            .regs
            .cr
            .borrow()
            .iter()
            .filter(|c| **c & CR_INHIBIT == 0)
            .count();
        assert_eq!(starts, 3);
    }

    #[test]
    fn device_asserts_its_line_and_pads_transfers() {
        let mut s = AxiSpi::new(FakeSpi::default(), 16, 2);
        s.init(embedded_hal::spi::MODE_0, false, false).unwrap();
        let mut dev = s.device(1).unwrap();
        let mut rd = [0u8; 2];
        dev.transaction(&mut [Operation::Transfer(&mut rd, &[0x9F, 0, 0])])
            .unwrap();
        assert_eq!(rd, [!0x9F, !0]);
        assert!(s.regs.ssr.borrow().contains(&!(1u32 << 1)));
        assert!(matches!(s.device(5), Err(SpiError::BadChipSelect(5))));
    }

    #[test]
    fn timeout_when_core_never_responds() {
        struct Dead;
        impl Mmio for Dead {
            fn len(&self) -> usize {
                0x1000
            }
            fn read32(&self, o: usize) -> Result<u32, MmioError> {
                Ok(if o == SPISR { SR_RX_EMPTY } else { 0 })
            }
            fn write32(&self, _: usize, _: u32) -> Result<(), MmioError> {
                Ok(())
            }
        }
        let mut s = AxiSpi::new(Dead, 16, 1);
        s.timeout = Duration::from_millis(5);
        assert!(matches!(s.write(&[1]), Err(SpiError::Timeout)));
    }

    #[test]
    fn registers_and_bits_match_the_hwh() {
        let h = |r, f| crate::hwh("pynq_z2_peripherals", "axi_quad_spi_0", r, f);
        for (r, o) in [
            ("SRR", SRR),
            ("SPICR", SPICR),
            ("SPISR", SPISR),
            ("SPI_DTR", SPIDTR),
            ("SPI_DRR", SPIDRR),
            ("SPI_SSR", SPISSR),
        ] {
            assert_eq!(h(r, None).0, o, "{r}");
        }
        for (f, m) in [
            ("LOOP", CR_LOOP),
            ("SPE", CR_SPE),
            ("Master", CR_MASTER),
            ("CPOL", CR_CPOL),
            ("CPHA", CR_CPHA),
            ("TX_FIFO_Reset", CR_TX_RST),
            ("RX_FIFO_Reset", CR_RX_RST),
            ("Manual_Slave_Select_Assertion_Enable", CR_MANUAL_SS),
            ("Master_Transaction_Inhibit", CR_INHIBIT),
            ("LSB_First", CR_LSB_FIRST),
        ] {
            assert_eq!(1u32 << h("SPICR", Some(f)).1, m, "{f}");
        }
        assert_eq!(1u32 << h("SPISR", Some("RX_Empty")).1, SR_RX_EMPTY);
    }
}
