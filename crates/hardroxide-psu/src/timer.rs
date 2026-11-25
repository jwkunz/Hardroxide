//! The generic timer: the system-wide counter every core reads, and the core's secure
//! physical timer (interrupt [`SECURE_PHYSICAL_TIMER`](crate::map::ppi::SECURE_PHYSICAL_TIMER)).
//!
//! The counter runs from the system counter, which `psu_init` (or the boot loader) starts;
//! its frequency is [`crate::clocks::timestamp_hz`]. The conversions are plain functions; the
//! register access is AArch64 only.

use core::time::Duration;

/// `ticks` of a `hz` counter in nanoseconds (saturating; 0 if `hz` is 0).
pub fn ticks_to_ns(ticks: u64, hz: u64) -> u64 {
    if hz == 0 {
        return 0;
    }
    u64::try_from(u128::from(ticks) * 1_000_000_000 / u128::from(hz)).unwrap_or(u64::MAX)
}

/// The number of ticks of a `hz` counter that `d` lasts (saturating).
pub fn duration_to_ticks(d: Duration, hz: u64) -> u64 {
    u64::try_from(d.as_nanos() * u128::from(hz) / 1_000_000_000).unwrap_or(u64::MAX)
}

#[cfg(target_arch = "aarch64")]
pub use hardware::{counter, frequency, start_one_shot, start_periodic, stop, tick};

#[cfg(target_arch = "aarch64")]
mod hardware {
    use core::arch::asm;
    use core::sync::atomic::{AtomicU64, Ordering};

    static PERIOD: AtomicU64 = AtomicU64::new(0);

    /// The system counter.
    pub fn counter() -> u64 {
        let v: u64;
        // SAFETY: reads the counter, after the instructions before it have completed.
        unsafe { asm!("isb", "mrs {0}, cntpct_el0", out(reg) v, options(nostack)) };
        v
    }

    /// What `CNTFRQ_EL0` says the counter's frequency is (set by [`crate::time::init`]).
    pub fn frequency() -> u64 {
        let v: u64;
        // SAFETY: reads a system register.
        unsafe { asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nomem, nostack)) };
        v
    }

    fn set_compare(value: u64) {
        // SAFETY: this core's secure timer is used only through this module.
        unsafe {
            asm!("msr cntps_cval_el1, {0}", "msr cntps_ctl_el1, {1}", "isb",
                 in(reg) value, in(reg) 1u64, options(nostack))
        };
    }

    /// Raise the timer interrupt every `ticks` counter ticks, without drift. The handler
    /// must call [`tick`].
    pub fn start_periodic(ticks: u64) {
        PERIOD.store(ticks.max(1), Ordering::Relaxed);
        set_compare(counter().wrapping_add(ticks.max(1)));
    }

    /// Raise the timer interrupt once, `ticks` from now. The handler must call [`stop`] or
    /// [`tick`].
    pub fn start_one_shot(ticks: u64) {
        PERIOD.store(0, Ordering::Relaxed);
        set_compare(counter().wrapping_add(ticks));
    }

    /// In the handler: arm the next period (counted from the last deadline, not from now),
    /// or stop a one-shot. Either way the interrupt line drops.
    pub fn tick() {
        let period = PERIOD.load(Ordering::Relaxed);
        if period == 0 {
            return stop();
        }
        let last: u64;
        // SAFETY: reads this core's timer.
        unsafe { asm!("mrs {0}, cntps_cval_el1", out(reg) last, options(nomem, nostack)) };
        set_compare(last.wrapping_add(period));
    }

    /// Stop the timer.
    pub fn stop() {
        // SAFETY: this core's secure timer is used only through this module.
        unsafe { asm!("msr cntps_ctl_el1, xzr", "isb", options(nostack)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_round_trip_at_the_board_frequency() {
        let hz = 33_333_328;
        assert_eq!(duration_to_ticks(Duration::from_millis(10), hz), 333_333);
        assert!(ticks_to_ns(333_333, hz).abs_diff(10_000_000) < 100);
        // A year of ticks does not overflow.
        let year = duration_to_ticks(Duration::from_secs(31_536_000), hz);
        assert!(ticks_to_ns(year, hz).abs_diff(31_536_000_000_000_000) < 1000);
        assert_eq!(ticks_to_ns(5, 0), 0);
    }
}
