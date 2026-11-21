//! A monotonic clock for drivers' timeouts, usable with and without `std`.
//!
//! Drivers measure timeouts with [`Instant`]. With the `std` feature it defaults to
//! `std::time::Instant`. On bare metal there is no operating system to ask, so the
//! platform registers a time source once at start-up with [`set_time_source`]; until
//! then [`Instant::now`] panics rather than let a timeout loop spin forever.

use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

/// Nanoseconds since an arbitrary, fixed epoch. Must be monotonic and non-decreasing.
pub type TimeSource = fn() -> u64;

static SOURCE: AtomicUsize = AtomicUsize::new(0);

/// Register the platform's clock. Idempotent; the last call wins. Safe to call from
/// any context (it is one atomic store).
pub fn set_time_source(source: TimeSource) {
    SOURCE.store(source as usize, Ordering::Release);
}

fn registered() -> Option<TimeSource> {
    match SOURCE.load(Ordering::Acquire) {
        0 => None,
        // SAFETY: the only writer is `set_time_source`, which stores a `TimeSource`
        // function pointer cast to `usize`; zero (no source) is handled above.
        p => Some(unsafe { core::mem::transmute::<usize, TimeSource>(p) }),
    }
}

#[cfg(feature = "std")]
fn std_nanos() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as u64
}

fn now_nanos() -> u64 {
    if let Some(f) = registered() {
        return f();
    }
    #[cfg(feature = "std")]
    {
        std_nanos()
    }
    #[cfg(not(feature = "std"))]
    {
        panic!(
            "hardroxide_mmio::time: no time source registered; call set_time_source() at start-up"
        )
    }
}

/// A point in monotonic time. Mirrors the part of `std::time::Instant` the drivers use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(u64);

impl Instant {
    pub fn now() -> Self {
        Instant(now_nanos())
    }
    /// Time elapsed since this instant (zero if the clock went backwards).
    pub fn elapsed(&self) -> Duration {
        Duration::from_nanos(now_nanos().saturating_sub(self.0))
    }
    /// Time from `earlier` to `self`, or zero.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }
    pub fn checked_add(&self, d: Duration) -> Option<Instant> {
        u64::try_from(d.as_nanos())
            .ok()
            .and_then(|n| self.0.checked_add(n))
            .map(Instant)
    }
}

/// Wait for at least `d`. With `std` this sleeps the thread; without it, it busy-waits
/// on the registered time source (an RTOS can later replace this with a blocking delay).
pub fn sleep(d: Duration) {
    #[cfg(feature = "std")]
    std::thread::sleep(d);
    #[cfg(not(feature = "std"))]
    {
        let t0 = Instant::now();
        while t0.elapsed() < d {
            core::hint::spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU64;

    static FAKE: AtomicU64 = AtomicU64::new(0);
    fn fake() -> u64 {
        FAKE.load(Ordering::SeqCst)
    }

    #[test]
    fn a_registered_source_drives_elapsed_time() {
        set_time_source(fake);
        FAKE.store(1_000, Ordering::SeqCst);
        let t = Instant::now();
        FAKE.store(1_000 + 2_500_000, Ordering::SeqCst);
        assert_eq!(t.elapsed(), Duration::from_nanos(2_500_000));
        // A clock that steps backwards never produces a huge duration.
        FAKE.store(0, Ordering::SeqCst);
        assert_eq!(t.elapsed(), Duration::ZERO);
        assert_eq!(
            t.checked_add(Duration::from_nanos(5))
                .unwrap()
                .duration_since(t),
            Duration::from_nanos(5)
        );
    }
}
