//! AXI Timer/Counter (`xilinx.com:ip:axi_timer`, PG079), timer 0 in
//! generate mode. Register offsets are asserted against the HWH in tests.

use embedded_hal::delay::DelayNs;
use hardroxide_mmio::{Mmio, MmioError};

pub const TCSR0: usize = 0x00;
pub const TLR0: usize = 0x04;
pub const TCR0: usize = 0x08;

const UDT: u32 = 1 << 1; // 1 = count down
const ARHT: u32 = 1 << 4; // auto reload / hold
const LOAD: u32 = 1 << 5;
const ENIT: u32 = 1 << 6;
const ENT: u32 = 1 << 7;
const TINT: u32 = 1 << 8; // write 1 to clear

pub struct AxiTimer<R: Mmio> {
    regs: R,
    clk_hz: u64,
}

impl<R: Mmio> AxiTimer<R> {
    /// `clk_hz` is the AXI clock driving the counter (HWH clock port).
    pub fn new(regs: R, clk_hz: u64) -> Self {
        Self { regs, clk_hz }
    }

    pub fn clock_hz(&self) -> u64 {
        self.clk_hz
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Start a down-counter that expires after `ticks` clocks. `periodic`
    /// reloads automatically; `irq` sets the interrupt enable bit.
    pub fn start(&self, ticks: u32, periodic: bool, irq: bool) -> Result<(), MmioError> {
        self.regs.write32(TCSR0, 0)?; // stop, generate mode
        self.regs.write32(TLR0, ticks)?;
        let base = UDT | if periodic { ARHT } else { 0 } | if irq { ENIT } else { 0 };
        self.regs.write32(TCSR0, base | LOAD)?; // load TLR into the counter
        self.regs.write32(TCSR0, (base & !LOAD) | TINT | ENT)?; // clear stale TINT, run
        Ok(())
    }

    pub fn stop(&self) -> Result<(), MmioError> {
        self.regs.modify32(TCSR0, |v| v & !ENT)
    }

    pub fn counter(&self) -> Result<u32, MmioError> {
        self.regs.read32(TCR0)
    }

    /// `true` once the counter expired. Does not clear the flag.
    pub fn expired(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(TCSR0)? & TINT != 0)
    }

    /// Clear the expiry/interrupt flag (write-1-to-clear).
    pub fn clear(&self) -> Result<(), MmioError> {
        self.regs.modify32(TCSR0, |v| v | TINT)
    }

    pub fn ticks_for_ns(&self, ns: u64) -> u64 {
        (ns as u128 * self.clk_hz as u128).div_ceil(1_000_000_000) as u64
    }

    /// Busy-wait for `ticks`, splitting into 32-bit chunks. Errors: MMIO only.
    pub fn delay_ticks(&self, mut ticks: u64) -> Result<(), MmioError> {
        while ticks > 0 {
            let chunk = ticks.min(u32::MAX as u64) as u32;
            self.start(chunk.max(2), false, false)?;
            while !self.expired()? {
                std::hint::spin_loop();
            }
            self.stop()?;
            ticks -= chunk as u64;
        }
        Ok(())
    }
}

/// Blocking delays. MMIO errors cannot be reported by `DelayNs`, so they panic;
/// use [`AxiTimer::delay_ticks`] to handle them.
impl<R: Mmio> DelayNs for AxiTimer<R> {
    fn delay_ns(&mut self, ns: u32) {
        self.delay_ticks(self.ticks_for_ns(ns as u64))
            .expect("timer MMIO failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn start_programs_load_then_run() {
        let be = MockBackend::new();
        let t = AxiTimer::new(be.map(0x4200_0000, 0x1000).unwrap(), 100_000_000);
        be.set_write1_clear_mask(0x4200_0000, TINT);
        t.start(1000, true, true).unwrap();
        assert_eq!(be.peek(0x4200_0004), 1000);
        assert_eq!(
            be.peek(0x4200_0000) & (ENT | UDT | ARHT | ENIT),
            ENT | UDT | ARHT | ENIT
        );
        assert_eq!(t.ticks_for_ns(1_000), 100);
        assert_eq!(t.ticks_for_ns(1), 1, "rounds up");
        assert!(!t.expired().unwrap());
        be.poke(0x4200_0000, TINT);
        assert!(t.expired().unwrap());
    }

    #[test]
    fn delay_returns_when_expired() {
        let be = MockBackend::new();
        let mut t = AxiTimer::new(be.map(0x4200_0000, 0x1000).unwrap(), 100_000_000);
        // Hardware that expires immediately: TINT is set and nothing clears it.
        be.set_write1_clear(0x4200_0000 + 0xFFC); // unrelated register
        let b2 = be.clone();
        let h = std::thread::spawn(move || {
            for _ in 0..1000 {
                b2.poke(0x4200_0000, b2.peek(0x4200_0000) | TINT);
                std::thread::sleep(std::time::Duration::from_micros(50));
            }
        });
        t.delay_ns(10_000);
        h.join().unwrap();
    }

    #[test]
    fn registers_and_bits_match_the_hwh() {
        let h = |r, f| crate::hwh("pynq_z2_peripherals", "axi_timer_0", r, f);
        assert_eq!(h("TCSR0", None).0, TCSR0);
        assert_eq!(h("TLR0", None).0, TLR0);
        assert_eq!(h("TCR0", None).0, TCR0);
        for (f, m) in [
            ("UDT0", UDT),
            ("ARHT0", ARHT),
            ("LOAD0", LOAD),
            ("ENIT0", ENIT),
            ("ENT0", ENT),
            ("T0INT", TINT),
        ] {
            assert_eq!(1u32 << h("TCSR0", Some(f)).1, m, "{f}");
        }
    }
}
