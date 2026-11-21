//! The Cortex-A9 MPCore timers: the 64-bit **global timer** (free-running, shared by both
//! cores; the system's time base) and the **private timer** (one per core; the kernel
//! tick). Both run at the private-peripheral clock (half the CPU clock, 325 MHz on the
//! PYNQ-Z2; see [`crate::clocks::periph_hz`]).

use core::time::Duration;
use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

// Global timer.
const GT_COUNT_LO: usize = 0x00;
const GT_COUNT_HI: usize = 0x04;
const GT_CTRL: usize = 0x08;
// Private timer.
const PT_LOAD: usize = 0x00;
const PT_COUNT: usize = 0x04;
const PT_CTRL: usize = 0x08;
const PT_ISR: usize = 0x0C;

/// A free-running 64-bit counter.
pub struct GlobalTimer<R: Mmio> {
    regs: R,
}

impl<R: Mmio> GlobalTimer<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    /// Start counting (a no-op if it already runs; the count is not reset).
    pub fn enable(&self) -> Result<()> {
        self.regs.modify32(GT_CTRL, |v| v | 1)
    }

    /// The 64-bit count, read consistently across the two halves.
    pub fn count(&self) -> Result<u64> {
        loop {
            let hi = self.regs.read32(GT_COUNT_HI)?;
            let lo = self.regs.read32(GT_COUNT_LO)?;
            if self.regs.read32(GT_COUNT_HI)? == hi {
                return Ok(u64::from(hi) << 32 | u64::from(lo));
            }
        }
    }
}

/// Convert a count of `hz`-rate ticks to nanoseconds without 128-bit arithmetic or overflow.
pub fn ticks_to_ns(ticks: u64, hz: u64) -> u64 {
    (ticks / hz) * 1_000_000_000 + (ticks % hz) * 1_000_000_000 / hz
}

/// Ticks of an `hz` clock in `d` (rounded down; saturates).
pub fn duration_to_ticks(d: Duration, hz: u64) -> u64 {
    let s = d.as_secs().saturating_mul(hz);
    let n = u64::from(d.subsec_nanos()) * hz / 1_000_000_000;
    s.saturating_add(n)
}

/// A 32-bit down-counter that can raise a periodic interrupt (PPI 29).
pub struct PrivateTimer<R: Mmio> {
    regs: R,
}

impl<R: Mmio> PrivateTimer<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    /// Count down from `ticks`, reloading it each time it reaches zero, with the interrupt
    /// enabled. The first expiry comes `ticks` clocks after this call and the period is
    /// exactly `ticks` clocks (the counter runs from the load value to zero inclusive, so
    /// the load register gets `ticks - 1`).
    pub fn start_periodic(&self, ticks: u32) -> Result<()> {
        self.regs.write32(PT_CTRL, 0)?;
        self.regs.write32(PT_ISR, 1)?;
        self.regs.write32(PT_LOAD, ticks.saturating_sub(1))?;
        self.regs.write32(PT_CTRL, 0b111) // enable | auto-reload | interrupt enable
    }

    /// Count down once; the interrupt comes `ticks` clocks after this call.
    pub fn start_one_shot(&self, ticks: u32) -> Result<()> {
        self.regs.write32(PT_CTRL, 0)?;
        self.regs.write32(PT_ISR, 1)?;
        self.regs.write32(PT_LOAD, ticks.saturating_sub(1))?;
        self.regs.write32(PT_CTRL, 0b101) // enable | interrupt enable
    }

    pub fn stop(&self) -> Result<()> {
        self.regs.write32(PT_CTRL, 0)
    }

    pub fn counter(&self) -> Result<u32> {
        self.regs.read32(PT_COUNT)
    }

    /// Acknowledge the timer event; call from the interrupt handler.
    pub fn clear_interrupt(&self) -> Result<()> {
        self.regs.write32(PT_ISR, 1)
    }

    pub fn event_pending(&self) -> Result<bool> {
        Ok(self.regs.read32(PT_ISR)? & 1 != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn tick_conversion_is_exact_and_does_not_overflow() {
        assert_eq!(ticks_to_ns(325_000_000, 325_000_000), 1_000_000_000);
        assert_eq!(ticks_to_ns(325, 325_000_000), 1_000);
        // 10 years of 325 MHz ticks still converts (a naive ticks * 1e9 would overflow u64).
        let ten_years = 10 * 365 * 86_400u64 * 325_000_000;
        assert_eq!(
            ticks_to_ns(ten_years, 325_000_000),
            10 * 365 * 86_400 * 1_000_000_000
        );
        assert_eq!(
            duration_to_ticks(Duration::from_millis(10), 325_000_000),
            3_250_000
        );
        assert_eq!(
            duration_to_ticks(Duration::from_secs(2), 325_000_000),
            650_000_000
        );
    }

    #[test]
    fn global_timer_reads_a_consistent_64_bit_value() {
        let be = MockBackend::new();
        let t = GlobalTimer::new(be.map(0xF8F0_0200, 0x20).unwrap());
        be.poke(0xF8F0_0204, 0x1);
        be.poke(0xF8F0_0200, 0xFFFF_FFF0);
        assert_eq!(t.count().unwrap(), 0x1_FFFF_FFF0);
        t.enable().unwrap();
        assert_eq!(be.peek(0xF8F0_0208) & 1, 1);
    }

    #[test]
    fn private_timer_programs_load_and_control() {
        let be = MockBackend::new();
        let t = PrivateTimer::new(be.map(0xF8F0_0600, 0x20).unwrap());
        t.start_periodic(325_000).unwrap();
        assert_eq!(
            be.peek(0xF8F0_0600),
            324_999,
            "the counter runs load..=0, so the load is one less"
        );
        assert_eq!(be.peek(0xF8F0_0608), 0b111);
        t.start_one_shot(5).unwrap();
        assert_eq!(be.peek(0xF8F0_0608), 0b101);
        t.stop().unwrap();
        assert_eq!(be.peek(0xF8F0_0608), 0);
        t.clear_interrupt().unwrap();
        assert_eq!(be.peek(0xF8F0_060C), 1);
    }
}
