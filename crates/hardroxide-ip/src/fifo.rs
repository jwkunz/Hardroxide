//! AXI4-Stream FIFO (`xilinx.com:ip:axi_fifo_mm_s`, PG080) in its AXI4-Lite
//! register mode, with packets moved by programmed I/O. Both directions use
//! 32-bit words, little-endian; the packet length in bytes is exact (the last
//! word is zero-padded on send and truncated on receive).
//! UNVERIFIED on hardware; offsets/bits are asserted against the HWH.

use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

pub const ISR: usize = 0x00;
pub const IER: usize = 0x04;
/// Transmit data FIFO reset.
pub const TDFR: usize = 0x08;
/// Transmit data FIFO vacancy, in words.
pub const TDFV: usize = 0x0C;
/// Transmit data FIFO write port.
pub const TDFD: usize = 0x10;
/// Transmit length in bytes; writing it starts the packet.
pub const TLR: usize = 0x14;
/// Receive data FIFO reset.
pub const RDFR: usize = 0x18;
/// Receive data FIFO occupancy, in words.
pub const RDFO: usize = 0x1C;
/// Receive data FIFO read port.
pub const RDFD: usize = 0x20;
/// Receive length in bytes; reading it begins a packet.
pub const RLR: usize = 0x24;
/// Whole-core reset.
pub const SRR: usize = 0x28;

const RESET_KEY: u32 = 0xA5;
const RLR_LEN_MASK: u32 = 0x007F_FFFF;

#[derive(Debug, thiserror::Error)]
pub enum FifoError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("packet of {len} bytes needs {need} words but the transmit FIFO has room for {have}")]
    NoRoom {
        len: usize,
        need: usize,
        have: usize,
    },
    #[error("empty packets cannot be sent")]
    EmptyPacket,
    #[error("received a {len}-byte packet into a {cap}-byte buffer; the packet was discarded")]
    BufferTooSmall { len: usize, cap: usize },
    #[error("timed out after {0:?}")]
    Timeout(Duration),
}

pub struct AxiFifo<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiFifo<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Reset the core and both data FIFOs and clear every interrupt flag.
    pub fn reset(&self) -> Result<(), FifoError> {
        self.regs.write32(SRR, RESET_KEY)?;
        self.regs.write32(TDFR, RESET_KEY)?;
        self.regs.write32(RDFR, RESET_KEY)?;
        self.regs.write32(ISR, 0xFFFF_FFFF)?;
        Ok(())
    }

    /// Words of room in the transmit FIFO.
    pub fn tx_vacancy(&self) -> Result<usize, FifoError> {
        Ok(self.regs.read32(TDFV)? as usize)
    }

    /// Words waiting in the receive FIFO (0 if no packet has completed).
    pub fn rx_occupancy(&self) -> Result<usize, FifoError> {
        Ok(self.regs.read32(RDFO)? as usize)
    }

    pub fn interrupt_status(&self) -> Result<u32, FifoError> {
        Ok(self.regs.read32(ISR)?)
    }

    /// Clear the status bits set in `mask`.
    pub fn clear_interrupts(&self, mask: u32) -> Result<(), FifoError> {
        Ok(self.regs.write32(ISR, mask)?)
    }

    /// Choose which status bits raise the interrupt output.
    pub fn set_interrupt_mask(&self, mask: u32) -> Result<(), FifoError> {
        Ok(self.regs.write32(IER, mask)?)
    }

    /// Queue one packet for transmission. Fails with `NoRoom` (nothing written)
    /// if the FIFO cannot hold it whole.
    pub fn send(&self, data: &[u8]) -> Result<(), FifoError> {
        if data.is_empty() {
            return Err(FifoError::EmptyPacket);
        }
        let need = data.len().div_ceil(4);
        let have = self.tx_vacancy()?;
        if need > have {
            return Err(FifoError::NoRoom {
                len: data.len(),
                need,
                have,
            });
        }
        for chunk in data.chunks(4) {
            let mut w = [0u8; 4];
            w[..chunk.len()].copy_from_slice(chunk);
            self.regs.write32(TDFD, u32::from_le_bytes(w))?;
        }
        self.regs.write32(TLR, data.len() as u32)?;
        Ok(())
    }

    /// Take the next complete packet if there is one. Never blocks.
    pub fn try_recv(&self) -> Result<Option<Vec<u8>>, FifoError> {
        if self.rx_occupancy()? == 0 {
            return Ok(None);
        }
        let len = (self.regs.read32(RLR)? & RLR_LEN_MASK) as usize;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len.div_ceil(4) {
            out.extend_from_slice(&self.regs.read32(RDFD)?.to_le_bytes());
        }
        out.truncate(len);
        Ok(Some(out))
    }

    /// Wait up to `timeout` for a packet.
    pub fn recv(&self, timeout: Duration) -> Result<Option<Vec<u8>>, FifoError> {
        let t0 = Instant::now();
        loop {
            if let Some(p) = self.try_recv()? {
                return Ok(Some(p));
            }
            if t0.elapsed() > timeout {
                return Ok(None);
            }
        }
    }

    /// Like [`recv`](Self::recv) but into `buf`, returning the packet length.
    /// A packet longer than `buf` is read out and discarded (the FIFO cannot
    /// be rewound) and reported as `BufferTooSmall`.
    pub fn recv_into(&self, buf: &mut [u8], timeout: Duration) -> Result<Option<usize>, FifoError> {
        match self.recv(timeout)? {
            None => Ok(None),
            Some(p) if p.len() <= buf.len() => {
                buf[..p.len()].copy_from_slice(&p);
                Ok(Some(p.len()))
            }
            Some(p) => Err(FifoError::BufferTooSmall {
                len: p.len(),
                cap: buf.len(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// Loopback model: a packet sent via TLR appears in the receive FIFO.
    #[derive(Default)]
    struct Loop {
        tx_words: RefCell<Vec<u32>>,
        rx_words: RefCell<VecDeque<u32>>,
        rx_lens: RefCell<VecDeque<u32>>,
        vacancy: RefCell<u32>,
        writes: RefCell<Vec<(usize, u32)>>,
    }

    impl Mmio for Loop {
        fn len(&self) -> usize {
            0x1000
        }
        fn read32(&self, o: usize) -> Result<u32, MmioError> {
            Ok(match o {
                TDFV => *self.vacancy.borrow(),
                RDFO => self.rx_words.borrow().len() as u32,
                RLR => self.rx_lens.borrow_mut().pop_front().unwrap_or(0),
                RDFD => self.rx_words.borrow_mut().pop_front().unwrap_or(0),
                _ => 0,
            })
        }
        fn write32(&self, o: usize, v: u32) -> Result<(), MmioError> {
            self.writes.borrow_mut().push((o, v));
            match o {
                TDFD => self.tx_words.borrow_mut().push(v),
                TLR => {
                    let words = self.tx_words.borrow_mut().drain(..).collect::<Vec<_>>();
                    self.rx_words.borrow_mut().extend(words);
                    self.rx_lens.borrow_mut().push_back(v);
                }
                _ => {}
            }
            Ok(())
        }
    }

    fn fifo(vacancy: u32) -> AxiFifo<Loop> {
        let l = Loop::default();
        *l.vacancy.borrow_mut() = vacancy;
        AxiFifo::new(l)
    }

    #[test]
    fn packet_round_trips_with_exact_length() {
        let f = fifo(512);
        f.send(b"hello fifo!").unwrap(); // 11 bytes -> 3 words
        let tx = f.regs().writes.borrow().clone();
        assert_eq!(tx.last(), Some(&(TLR, 11)), "length written last");
        assert_eq!(tx.iter().filter(|w| w.0 == TDFD).count(), 3);
        assert_eq!(f.try_recv().unwrap().unwrap(), b"hello fifo!");
        assert_eq!(f.try_recv().unwrap(), None);
    }

    #[test]
    fn send_refuses_when_there_is_no_room_and_writes_nothing() {
        let f = fifo(2);
        let e = f.send(&[0; 12]).unwrap_err();
        assert!(matches!(
            e,
            FifoError::NoRoom {
                need: 3,
                have: 2,
                ..
            }
        ));
        assert!(f.regs().writes.borrow().is_empty());
        assert!(matches!(f.send(&[]), Err(FifoError::EmptyPacket)));
    }

    #[test]
    fn small_buffer_discards_the_packet() {
        let f = fifo(64);
        f.send(&[7; 8]).unwrap();
        let mut small = [0u8; 4];
        let e = f
            .recv_into(&mut small, Duration::from_millis(5))
            .unwrap_err();
        assert!(matches!(e, FifoError::BufferTooSmall { len: 8, cap: 4 }));
        assert_eq!(f.rx_occupancy().unwrap(), 0);
    }

    #[test]
    fn recv_times_out_when_idle() {
        let f = fifo(64);
        assert_eq!(f.recv(Duration::from_millis(5)).unwrap(), None);
    }

    #[test]
    fn reset_writes_the_keys() {
        let f = fifo(64);
        f.reset().unwrap();
        let w = f.regs().writes.borrow().clone();
        assert!(w.contains(&(SRR, 0xA5)) && w.contains(&(TDFR, 0xA5)) && w.contains(&(RDFR, 0xA5)));
    }

    #[test]
    fn registers_match_the_hwh() {
        let h = |r| crate::hwh("pynq_z2_fifo", "axi_fifo_mm_s_0", r, None).0;
        for (r, o) in [
            ("ISR", ISR),
            ("IER", IER),
            ("TDFR", TDFR),
            ("TDFV", TDFV),
            ("TDFD", TDFD),
            ("TLR", TLR),
            ("RDFR", RDFR),
            ("RDFO", RDFO),
            ("RDFD", RDFD),
            ("RLR", RLR),
            ("SRR", SRR),
        ] {
            assert_eq!(h(r), o, "{r}");
        }
    }
}
