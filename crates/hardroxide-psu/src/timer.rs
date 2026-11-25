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

/// The counts between successive deadlines of a timer that fires `rate_hz` times a second from a `counter_hz` counter,
/// exactly on average: a counter rate that the timer rate does not divide (33,333,333 Hz at 1 kHz) is spread over the
/// periods, one count at a time. `carried` is the remainder left over, 0 to begin with. A rate of 0 is taken as 1.
pub fn next_period(counter_hz: u64, rate_hz: u64, carried: &mut u64) -> u64 {
    let rate = rate_hz.max(1);
    *carried += counter_hz % rate;
    let extra = u64::from(*carried >= rate);
    *carried -= extra * rate;
    (counter_hz / rate + extra).max(1)
}

#[cfg(target_arch = "aarch64")]
pub use hardware::{counter, frequency, start_one_shot, start_periodic, start_rate, stop, tick};

#[cfg(target_arch = "aarch64")]
mod hardware {
    use core::arch::asm;
    use core::sync::atomic::{AtomicU64, Ordering};

    // A periodic timer fires `RATE` times per `SPAN` counts: for a period in counts, once per `SPAN` = that many; for a rate,
    // `RATE` times per second's worth of counts. `SPAN` of 0 marks a one-shot.
    static SPAN: AtomicU64 = AtomicU64::new(0);
    static RATE: AtomicU64 = AtomicU64::new(1);
    static CARRIED: AtomicU64 = AtomicU64::new(0);

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

    /// The counts to the next deadline.
    fn period() -> u64 {
        let mut carried = CARRIED.load(Ordering::Relaxed);
        let p = super::next_period(
            SPAN.load(Ordering::Relaxed),
            RATE.load(Ordering::Relaxed),
            &mut carried,
        );
        CARRIED.store(carried, Ordering::Relaxed);
        p
    }

    fn start(span: u64, rate: u64) {
        SPAN.store(span, Ordering::Relaxed);
        RATE.store(rate, Ordering::Relaxed);
        CARRIED.store(0, Ordering::Relaxed);
        set_compare(counter().wrapping_add(period()));
    }

    /// Raise the timer interrupt every `ticks` counter ticks, without drift. The handler
    /// must call [`tick`].
    pub fn start_periodic(ticks: u64) {
        start(ticks.max(1), 1);
    }

    /// Raise the timer interrupt `rate_hz` times a second, `counter_hz` being the counter's
    /// frequency, exactly on average even if one does not divide the other (see
    /// [`next_period`](super::next_period)). The handler must call [`tick`].
    pub fn start_rate(counter_hz: u64, rate_hz: u64) {
        start(counter_hz.max(1), rate_hz.max(1));
    }

    /// Raise the timer interrupt once, `ticks` from now. The handler must call [`stop`] or
    /// [`tick`].
    pub fn start_one_shot(ticks: u64) {
        SPAN.store(0, Ordering::Relaxed);
        set_compare(counter().wrapping_add(ticks));
    }

    /// In the handler: arm the next period (counted from the last deadline, not from now),
    /// or stop a one-shot. Either way the interrupt line drops.
    pub fn tick() {
        if SPAN.load(Ordering::Relaxed) == 0 {
            return stop();
        }
        let last: u64;
        // SAFETY: reads this core's timer.
        unsafe { asm!("mrs {0}, cntps_cval_el1", out(reg) last, options(nomem, nostack)) };
        set_compare(last.wrapping_add(period()));
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
    fn a_rate_the_counter_does_not_divide_is_exact_over_a_second() {
        // The RFSoC 4x2's counter and the kernel's usual tick: truncating to 33,333 counts would run 10 ppm fast.
        for (hz, rate) in [
            (33_333_333u64, 1000u64),
            (33_333_333, 97),
            (100, 7),
            (325_000_000, 1000),
        ] {
            let mut carried = 0;
            let periods: [u64; 2] = core::array::from_fn(|_| {
                (0..rate).map(|_| next_period(hz, rate, &mut carried)).sum()
            });
            assert_eq!(periods, [hz, hz], "{hz} Hz at {rate} Hz");
            // No period is off by more than a count.
            let mut carried = 0;
            assert!(
                (0..rate).all(|_| next_period(hz, rate, &mut carried).abs_diff(hz / rate) <= 1)
            );
        }
        // A plain period in counts is a rate of once per that many.
        let mut carried = 0;
        assert_eq!(next_period(333_333, 1, &mut carried), 333_333);
        assert_eq!(next_period(1, 1000, &mut carried), 1);
    }

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
