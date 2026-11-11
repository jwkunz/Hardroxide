//! Vitis HLS IP with an AXI-Lite control interface (`ap_ctrl_hs` /
//! `ap_ctrl_chain`). Control block layout (UG1399):
//! `0x00` CTRL, `0x04` GIER, `0x08` IP_IER, `0x0C` IP_ISR; user registers
//! start at `0x10`.

use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

pub const CTRL: usize = 0x00;
pub const GIER: usize = 0x04;
pub const IP_IER: usize = 0x08;
pub const IP_ISR: usize = 0x0C;
/// First user-register offset.
pub const USER_BASE: usize = 0x10;

const AP_START: u32 = 1 << 0;
const AP_DONE: u32 = 1 << 1;
const AP_IDLE: u32 = 1 << 2;
const AP_READY: u32 = 1 << 3;
const AUTO_RESTART: u32 = 1 << 7;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("timed out after {0:?} waiting for ap_done")]
    Timeout(Duration),
}

/// Control helpers plus raw access to the register window.
pub struct HlsIp<R: Mmio> {
    regs: R,
}

impl<R: Mmio> HlsIp<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    fn ctrl(&self) -> Result<u32, MmioError> {
        self.regs.read32(CTRL)
    }

    /// Set `ap_start`. The hardware clears it when the core accepts the call.
    pub fn start(&self) -> Result<(), MmioError> {
        // Preserve auto_restart; do not write the read-to-clear bits as 1.
        let v = self.ctrl()? & AUTO_RESTART;
        self.regs.write32(CTRL, v | AP_START)
    }

    pub fn is_idle(&self) -> Result<bool, MmioError> {
        Ok(self.ctrl()? & AP_IDLE != 0)
    }

    /// `ap_ready`: the core can accept new inputs. Clears on read.
    pub fn is_ready(&self) -> Result<bool, MmioError> {
        Ok(self.ctrl()? & AP_READY != 0)
    }

    /// `ap_done`: the call finished. Clears on read, so a `true` is consumed.
    pub fn is_done(&self) -> Result<bool, MmioError> {
        Ok(self.ctrl()? & AP_DONE != 0)
    }

    pub fn set_auto_restart(&self, on: bool) -> Result<(), MmioError> {
        self.regs.modify32(CTRL, |v| {
            let v = v & AUTO_RESTART; // only the writable bit survives
            if on { v | AUTO_RESTART } else { v & !AUTO_RESTART }
        })
    }

    /// Poll until `ap_done`, or time out.
    pub fn wait_done(&self, timeout: Duration) -> Result<(), HlsError> {
        let t0 = Instant::now();
        loop {
            if self.is_done()? {
                return Ok(());
            }
            if t0.elapsed() >= timeout {
                return Err(HlsError::Timeout(timeout));
            }
            std::hint::spin_loop();
        }
    }

    /// Enable the `ap_done` interrupt (GIER + IP_IER bit 0).
    pub fn enable_done_interrupt(&self) -> Result<(), MmioError> {
        self.regs.write32(GIER, 1)?;
        self.regs.modify32(IP_IER, |v| v | 1)
    }

    pub fn disable_interrupts(&self) -> Result<(), MmioError> {
        self.regs.write32(GIER, 0)?;
        self.regs.write32(IP_IER, 0)
    }

    /// Acknowledge interrupt status bits (toggle-on-write), returning the
    /// bits that were pending.
    pub fn ack_interrupts(&self) -> Result<u32, MmioError> {
        let st = self.regs.read32(IP_ISR)? & 0x3;
        if st != 0 {
            self.regs.write32(IP_ISR, st)?;
        }
        Ok(st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn start_done_and_interrupts() {
        let be = MockBackend::new();
        let ip = HlsIp::new(be.map(0x4000_0000, 0x1000).unwrap());
        ip.set_auto_restart(true).unwrap();
        ip.start().unwrap();
        assert_eq!(be.peek(0x4000_0000), AP_START | AUTO_RESTART);
        be.poke(0x4000_0000, AP_DONE | AP_IDLE);
        assert!(ip.is_done().unwrap() && ip.is_idle().unwrap());
        ip.wait_done(Duration::from_millis(10)).unwrap();
        be.poke(0x4000_0000, 0);
        assert!(matches!(ip.wait_done(Duration::from_millis(5)), Err(HlsError::Timeout(_))));
        ip.enable_done_interrupt().unwrap();
        assert_eq!((be.peek(0x4000_0004), be.peek(0x4000_0008)), (1, 1));
        be.poke(0x4000_000C, 1);
        assert_eq!(ip.ack_interrupts().unwrap(), 1);
        assert_eq!(be.peek(0x4000_000C), 1);
    }
}
