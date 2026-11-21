//! The system time base: the global timer registered as the clock the drivers use for
//! timeouts ([`hardroxide_mmio::time`]).

use crate::map::{GLOBAL_TIMER_BASE, GLOBAL_TIMER_SIZE};
use crate::timer::{GlobalTimer, ticks_to_ns};
use core::sync::atomic::{AtomicU64, Ordering};
use hardroxide_mmio::{MmioBackend, PhysMem};

static HZ: AtomicU64 = AtomicU64::new(0);

fn global_ns() -> u64 {
    let hz = HZ.load(Ordering::Relaxed);
    // SAFETY: `init` made this window the global timer's, read-only here.
    let be = unsafe { PhysMem::new() };
    match be
        .map(GLOBAL_TIMER_BASE as u64, GLOBAL_TIMER_SIZE)
        .ok()
        .and_then(|r| GlobalTimer::new(r).count().ok())
    {
        Some(t) if hz != 0 => ticks_to_ns(t, hz),
        _ => 0,
    }
}

/// Start the global timer and register it as the time source of `hardroxide-mmio`, so
/// the drivers' timeouts and [`hardroxide_mmio::time::sleep`] work. `periph_hz` is the
/// private-peripheral clock ([`crate::clocks::periph_hz`]).
///
/// # Safety
/// The global timer window must not be used by other code that changes its control
/// register (the 64-bit counter is only ever read here).
pub unsafe fn init(periph_hz: u64) {
    // SAFETY: as documented above.
    let be = unsafe { PhysMem::new() };
    if let Ok(r) = be.map(GLOBAL_TIMER_BASE as u64, GLOBAL_TIMER_SIZE) {
        let _ = GlobalTimer::new(r).enable();
    }
    HZ.store(periph_hz, Ordering::Relaxed);
    hardroxide_mmio::time::set_time_source(global_ns);
}

/// Nanoseconds since the global timer started counting.
pub fn now_ns() -> u64 {
    global_ns()
}
