//! The system time base: the generic timer's counter registered as the clock the drivers
//! use for timeouts ([`hardroxide_mmio::time`]).

use crate::timer::{self, ticks_to_ns};
use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

static HZ: AtomicU64 = AtomicU64::new(0);

fn counter_ns() -> u64 {
    ticks_to_ns(timer::counter(), HZ.load(Ordering::Relaxed))
}

/// Record the counter's frequency (in `CNTFRQ_EL0`, which software at EL3 must set) and
/// register the counter as the time source of `hardroxide-mmio`, so the drivers' timeouts
/// and [`hardroxide_mmio::time::sleep`] work. `counter_hz` is
/// [`crate::clocks::timestamp_hz`].
///
/// The system counter must be running: `psu_init` and the boot loader start it.
///
/// # Safety
/// Call at EL3 on core 0, before anything that reads the time.
pub unsafe fn init(counter_hz: u64) {
    // SAFETY: `CNTFRQ_EL0` is writable at EL3, as the caller guarantees.
    unsafe { asm!("msr cntfrq_el0, {0}", "isb", in(reg) counter_hz, options(nostack)) };
    HZ.store(counter_hz, Ordering::Relaxed);
    hardroxide_mmio::time::set_time_source(counter_ns);
}

/// Nanoseconds since the system counter started.
pub fn now_ns() -> u64 {
    counter_ns()
}
