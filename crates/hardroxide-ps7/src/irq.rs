//! A table of interrupt handlers, indexed by GIC interrupt ID.
//!
//! [`register`] installs a plain function for an ID; the IRQ vector entry calls
//! [`dispatch`] after acknowledging the interrupt. The table is a set of atomics, so it
//! needs no lock and can be written from any context.

use crate::map::GIC_IRQS;
use core::sync::atomic::{AtomicUsize, Ordering};

static HANDLERS: [AtomicUsize; GIC_IRQS] = [const { AtomicUsize::new(0) }; GIC_IRQS];
static UNHANDLED: AtomicUsize = AtomicUsize::new(0);

/// An interrupt service routine. It runs with IRQs masked, on the IRQ stack.
pub type Handler = fn();

/// Install `handler` for interrupt `id`, replacing any earlier one. Returns `false` if
/// `id` is out of range. This does not enable the interrupt in the GIC.
pub fn register(id: u32, handler: Handler) -> bool {
    match HANDLERS.get(id as usize) {
        Some(slot) => {
            slot.store(handler as usize, Ordering::Release);
            true
        }
        None => false,
    }
}

/// Remove the handler for `id`.
pub fn unregister(id: u32) {
    if let Some(slot) = HANDLERS.get(id as usize) {
        slot.store(0, Ordering::Release);
    }
}

/// Run the handler registered for `id`; returns `false` (and counts it) if there is none.
pub fn dispatch(id: u32) -> bool {
    let p = HANDLERS
        .get(id as usize)
        .map_or(0, |s| s.load(Ordering::Acquire));
    if p == 0 {
        UNHANDLED.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    // SAFETY: the only values stored are `Handler` function pointers cast to `usize`
    // by `register`; zero (no handler) is handled above.
    let f: Handler = unsafe { core::mem::transmute::<usize, Handler>(p) };
    f();
    true
}

/// How many interrupts arrived with no handler registered.
pub fn unhandled_count() -> usize {
    UNHANDLED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU32;

    static HITS: AtomicU32 = AtomicU32::new(0);
    fn h() {
        HITS.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn dispatch_calls_the_registered_handler_only() {
        assert!(!dispatch(70));
        assert!(unhandled_count() >= 1);
        assert!(register(70, h));
        assert!(dispatch(70));
        assert_eq!(HITS.load(Ordering::SeqCst), 1);
        unregister(70);
        assert!(!dispatch(70));
        assert!(!register(96, h));
        assert!(!dispatch(1000));
    }
}
