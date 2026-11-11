//! AXI DMA (`xilinx.com:ip:axi_dma`) in **direct register (simple) mode**
//! (PG021). Scatter-gather is not supported.
//!
//! Buffers are moved into a [`Transfer`] and only returned when the channel
//! is known to be stopped, so the CPU can never touch memory the hardware may
//! still be accessing.

use hardroxide_dma::DmaBuffer;
use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Memory to stream (MM2S): the PL reads the buffer.
    MemToStream,
    /// Stream to memory (S2MM): the PL writes the buffer.
    StreamToMem,
}

struct Regs {
    cr: usize,
    sr: usize,
    addr: usize,
    addr_msb: usize,
    len: usize,
}

const MM2S: Regs = Regs {
    cr: 0x00,
    sr: 0x04,
    addr: 0x18,
    addr_msb: 0x1C,
    len: 0x28,
};
const S2MM: Regs = Regs {
    cr: 0x30,
    sr: 0x34,
    addr: 0x48,
    addr_msb: 0x4C,
    len: 0x58,
};

// DMACR
const CR_RS: u32 = 1 << 0;
const CR_RESET: u32 = 1 << 2;
const CR_IOC_IRQ_EN: u32 = 1 << 12;
const CR_ERR_IRQ_EN: u32 = 1 << 14;
// DMASR
const SR_HALTED: u32 = 1 << 0;
const SR_IDLE: u32 = 1 << 1;
const SR_ERRORS: u32 = 0b111 << 4; // DMAIntErr, DMASlvErr, DMADecErr
const SR_IOC: u32 = 1 << 12;
const SR_W1C: u32 = 0b111 << 12; // IOC, Dly, Err irq flags

#[derive(Debug, thiserror::Error)]
pub enum DmaIpError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("transfer length {len} must be 1..={max} and fit the {buf}-byte buffer")]
    BadLength { len: usize, max: usize, buf: usize },
    #[error("channel is busy (DMASR = {0:#x})")]
    Busy(u32),
    #[error("DMA error flags set (DMASR = {0:#x})")]
    Hardware(u32),
    #[error("timed out after {0:?} (channel halted)")]
    Timeout(Duration),
    #[error("channel did not halt/reset in time")]
    StuckChannel,
}

/// A failed `start`: the buffer is handed back untouched.
pub struct StartError<B> {
    pub error: DmaIpError,
    pub buffer: B,
}

impl<B> std::fmt::Debug for StartError<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StartError({})", self.error)
    }
}

pub struct AxiDma<R: Mmio> {
    regs: R,
    max_len: usize,
    irq: bool,
}

impl<R: Mmio> AxiDma<R> {
    /// `length_width` is the IP's "Width of Buffer Length Register"
    /// (HWH `C_SG_LENGTH_WIDTH`; the Vivado default is 14).
    pub fn with_length_width(regs: R, length_width: u32) -> Self {
        let max_len = (1usize << length_width.clamp(8, 26)) - 1;
        Self {
            regs,
            max_len,
            irq: false,
        }
    }

    pub fn new(regs: R) -> Self {
        Self::with_length_width(regs, 14)
    }

    /// Enable completion/error interrupt generation for later transfers.
    pub fn with_interrupts(mut self, on: bool) -> Self {
        self.irq = on;
        self
    }

    pub fn max_transfer_len(&self) -> usize {
        self.max_len
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    fn r(dir: Direction) -> &'static Regs {
        match dir {
            Direction::MemToStream => &MM2S,
            Direction::StreamToMem => &S2MM,
        }
    }

    /// Soft-reset both channels and wait for the reset bits to clear.
    pub fn reset(&self) -> Result<(), DmaIpError> {
        for r in [&MM2S, &S2MM] {
            self.regs.write32(r.cr, CR_RESET)?;
        }
        let t0 = Instant::now();
        for r in [&MM2S, &S2MM] {
            while self.regs.read32(r.cr)? & CR_RESET != 0 {
                if t0.elapsed() > Duration::from_millis(100) {
                    return Err(DmaIpError::StuckChannel);
                }
            }
        }
        Ok(())
    }

    /// Stop a channel (clear RS) and wait until it reports halted.
    fn halt(&self, dir: Direction) -> Result<(), DmaIpError> {
        let r = Self::r(dir);
        self.regs.modify32(r.cr, |v| v & !CR_RS)?;
        let t0 = Instant::now();
        while self.regs.read32(r.sr)? & SR_HALTED == 0 {
            if t0.elapsed() > Duration::from_millis(100) {
                return Err(DmaIpError::StuckChannel);
            }
        }
        Ok(())
    }

    /// Start a transfer of `len` bytes, taking ownership of `buf`.
    pub fn start<B: DmaBuffer>(
        &self,
        dir: Direction,
        buf: B,
        len: usize,
    ) -> Result<Transfer<'_, R, B>, StartError<B>> {
        match self.start_inner(dir, &buf, len) {
            Ok(()) => Ok(Transfer {
                dma: self,
                dir,
                buf: Some(buf),
            }),
            Err(error) => Err(StartError { error, buffer: buf }),
        }
    }

    fn start_inner<B: DmaBuffer>(
        &self,
        dir: Direction,
        buf: &B,
        len: usize,
    ) -> Result<(), DmaIpError> {
        if len == 0 || len > self.max_len || len > buf.len() {
            return Err(DmaIpError::BadLength {
                len,
                max: self.max_len,
                buf: buf.len(),
            });
        }
        let r = Self::r(dir);
        let sr = self.regs.read32(r.sr)?;
        if sr & (SR_HALTED | SR_IDLE) == 0 {
            return Err(DmaIpError::Busy(sr));
        }
        let irq = if self.irq {
            CR_IOC_IRQ_EN | CR_ERR_IRQ_EN
        } else {
            0
        };
        self.regs.write32(r.cr, CR_RS | irq)?;
        self.regs.write32(r.sr, SR_W1C)?; // clear stale flags (write-1-to-clear)
        let phys = buf.phys_addr();
        self.regs.write32(r.addr, phys as u32)?;
        self.regs.write32(r.addr_msb, (phys >> 32) as u32)?;
        // Writing LENGTH starts the transfer; it must come last.
        self.regs.write32(r.len, len as u32)?;
        Ok(())
    }
}

/// An in-flight transfer. Owns the buffer until completion.
pub struct Transfer<'d, R: Mmio, B: DmaBuffer> {
    dma: &'d AxiDma<R>,
    dir: Direction,
    buf: Option<B>,
}

impl<R: Mmio, B: DmaBuffer> Transfer<'_, R, B> {
    /// Non-blocking completion check. `Ok(true)` when finished successfully.
    pub fn poll(&self) -> Result<bool, DmaIpError> {
        let r = AxiDma::<R>::r(self.dir);
        let sr = self.dma.regs.read32(r.sr)?;
        if sr & SR_ERRORS != 0 {
            return Err(DmaIpError::Hardware(sr));
        }
        Ok(sr & SR_IOC != 0)
    }

    /// Wait for completion and get the buffer back. On error or timeout the
    /// channel is halted first, then the error is returned and the buffer is
    /// dropped (hardware may have written part of it, and the caller no
    /// longer has a use for a half-filled buffer; use `poll` for finer
    /// control). After an interrupt, `wait(Duration::ZERO)` collects the
    /// result and clears the status flags.
    pub fn wait(mut self, timeout: Duration) -> Result<B, DmaIpError> {
        let t0 = Instant::now();
        loop {
            match self.poll() {
                Ok(true) => {
                    let r = AxiDma::<R>::r(self.dir);
                    self.dma.regs.write32(r.sr, SR_W1C)?;
                    return Ok(self.buf.take().expect("buffer present until completion"));
                }
                Ok(false) => {}
                Err(e) => return Err(self.abort(e)),
            }
            if t0.elapsed() >= timeout {
                return Err(self.abort(DmaIpError::Timeout(timeout)));
            }
            std::hint::spin_loop();
        }
    }

    /// Halt the channel; if it cannot be confirmed halted, leak the buffer so
    /// hardware can never write freed memory.
    fn abort(&mut self, err: DmaIpError) -> DmaIpError {
        match self.dma.halt(self.dir) {
            Ok(()) => err, // buf dropped with `self`
            Err(stuck) => {
                if let Some(b) = self.buf.take() {
                    std::mem::forget(b);
                }
                stuck
            }
        }
    }
}

impl<R: Mmio, B: DmaBuffer> std::fmt::Debug for Transfer<'_, R, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transfer({:?})", self.dir)
    }
}

impl<R: Mmio, B: DmaBuffer> Drop for Transfer<'_, R, B> {
    fn drop(&mut self) {
        if self.buf.is_some() {
            // Abandoned in flight: stop the channel before freeing the buffer.
            let _ = self.abort(DmaIpError::StuckChannel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_dma::MockBuffer;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const BASE: u64 = 0x4040_0000;

    fn setup() -> (MockBackend, AxiDma<hardroxide_mmio::MockRegion>) {
        let be = MockBackend::new();
        be.poke(BASE + 0x04, SR_HALTED); // MM2S halted
        be.poke(BASE + 0x34, SR_HALTED);
        be.set_write1_clear(BASE + 0x04);
        be.set_write1_clear(BASE + 0x34);
        let dma = AxiDma::with_length_width(be.map(BASE, 0x1000).unwrap(), 23);
        (be, dma)
    }

    #[test]
    fn mm2s_programs_registers_in_order_and_completes() {
        let (be, dma) = setup();
        let buf = MockBuffer::new(4096, 0x1F00_0000);
        let t = dma.start(Direction::MemToStream, buf, 1024).unwrap();
        assert_eq!(be.peek(BASE) & CR_RS, CR_RS);
        assert_eq!(
            (be.peek(BASE + 0x18), be.peek(BASE + 0x28)),
            (0x1F00_0000, 1024)
        );
        let log = be.log();
        let last = log.last().unwrap();
        assert!(
            matches!(last, hardroxide_mmio::MockAccess::Write { addr, value: 1024 } if *addr == BASE + 0x28),
            "LENGTH written last"
        );
        assert!(!t.poll().unwrap());
        let b2 = be.clone();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            b2.poke(BASE + 0x04, SR_IDLE | SR_IOC);
        });
        let buf = t.wait(Duration::from_secs(1)).unwrap();
        h.join().unwrap();
        assert_eq!(buf.len(), 4096);
    }

    #[test]
    fn s2mm_uses_s2mm_registers() {
        let (be, dma) = setup();
        let t = dma
            .start(Direction::StreamToMem, MockBuffer::new(64, 0x2000_0000), 64)
            .unwrap();
        assert_eq!(
            (be.peek(BASE + 0x48), be.peek(BASE + 0x58)),
            (0x2000_0000, 64)
        );
        drop(t); // abandoned: halts (mock reports halted) without panic
        assert_eq!(be.peek(BASE + 0x30) & CR_RS, 0);
    }

    #[test]
    fn rejects_bad_length_and_busy_and_returns_buffer() {
        let (be, dma) = setup();
        let e = dma
            .start(Direction::MemToStream, MockBuffer::new(16, 0), 32)
            .unwrap_err();
        assert!(matches!(e.error, DmaIpError::BadLength { .. }));
        assert_eq!(e.buffer.len(), 16);
        assert!(
            dma.start(Direction::MemToStream, MockBuffer::new(16, 0), 0)
                .is_err()
        );
        be.poke(BASE + 0x04, 0); // neither halted nor idle
        assert!(matches!(
            dma.start(Direction::MemToStream, MockBuffer::new(16, 0), 8)
                .unwrap_err()
                .error,
            DmaIpError::Busy(0)
        ));
    }

    #[test]
    fn hardware_error_and_timeout_halt_the_channel() {
        let (be, dma) = setup();
        let t = dma
            .start(Direction::MemToStream, MockBuffer::new(16, 0), 8)
            .unwrap();
        be.poke(BASE + 0x04, SR_HALTED | (1 << 6)); // DMADecErr
        assert!(matches!(
            t.wait(Duration::from_millis(10)),
            Err(DmaIpError::Hardware(_))
        ));
        assert_eq!(be.peek(BASE) & CR_RS, 0);
        be.poke(BASE + 0x04, SR_HALTED);
        let t = dma
            .start(Direction::MemToStream, MockBuffer::new(16, 0), 8)
            .unwrap();
        assert!(matches!(
            t.wait(Duration::from_millis(5)),
            Err(DmaIpError::Timeout(_))
        ));
    }

    #[test]
    fn register_offsets_match_the_hwh() {
        let x = hardroxide_hwh::Xsa::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/pynq_z2_dma_loopback.xsa"
        ))
        .unwrap();
        let dma = x.design.instance("axi_dma_0").unwrap();
        let off = |n: &str| {
            dma.address_blocks[0]
                .registers
                .iter()
                .find(|r| r.name == n)
                .unwrap()
                .offset as usize
        };
        for (r, p) in [("MM2S", &MM2S), ("S2MM", &S2MM)] {
            assert_eq!(off(&format!("{r}_DMACR")), p.cr);
            assert_eq!(off(&format!("{r}_DMASR")), p.sr);
            assert_eq!(off(&format!("{r}_LENGTH")), p.len);
        }
        assert_eq!(off("MM2S_SA"), MM2S.addr);
        assert_eq!(off("MM2S_SA_MSB"), MM2S.addr_msb);
        assert_eq!(off("S2MM_DA"), S2MM.addr);
        assert_eq!(off("S2MM_DA_MSB"), S2MM.addr_msb);
        let f = |reg: &str, field: &str| {
            let r = dma.address_blocks[0]
                .registers
                .iter()
                .find(|r| r.name == reg)
                .unwrap();
            let f = r
                .fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(field))
                .unwrap_or_else(|| {
                    panic!(
                        "{reg}.{field}: {:?}",
                        r.fields.iter().map(|f| &f.name).collect::<Vec<_>>()
                    )
                });
            f.bit_offset
        };
        assert_eq!(1u32 << f("MM2S_DMACR", "RS"), CR_RS);
        assert_eq!(1u32 << f("MM2S_DMACR", "Reset"), CR_RESET);
        assert_eq!(1u32 << f("MM2S_DMASR", "Halted"), SR_HALTED);
        assert_eq!(1u32 << f("MM2S_DMASR", "Idle"), SR_IDLE);
    }
}
