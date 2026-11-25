//! A table of interrupt handlers, indexed by GIC interrupt ID.
//!
//! [`register`] installs a plain function for an ID; the IRQ vector entry calls
//! [`dispatch`] after acknowledging the interrupt. The table is a set of atomics, so it
//! needs no lock and can be written from any context.
//!
//! Besides plain handlers there is an **event mode** ([`register_event`]) that mirrors
//! what a Linux UIO device does for the drivers: when the interrupt fires it is masked in
//! the GIC and counted; code waits for the count to change ([`take_event`]) and unmasks it
//! again when it is ready for the next one. The vector entry also masks any interrupt that
//! arrives with nothing registered, so a level-triggered line cannot storm.

use crate::map::GIC_IRQS;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

static HANDLERS: [AtomicUsize; GIC_IRQS] = [const { AtomicUsize::new(0) }; GIC_IRQS];
static UNHANDLED: AtomicUsize = AtomicUsize::new(0);
/// Marks an ID as event mode in `HANDLERS` (a real function pointer is never 1).
const EVENT: usize = 1;
static EVENTS: [AtomicU32; GIC_IRQS] = [const { AtomicU32::new(0) }; GIC_IRQS];
static CONSUMED: [AtomicU32; GIC_IRQS] = [const { AtomicU32::new(0) }; GIC_IRQS];

/// What [`dispatch`] did with an interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dispatched {
    /// A registered handler ran.
    Handler,
    /// The interrupt is in event mode and was counted. The caller must mask it in the GIC.
    Event,
    /// Nothing was registered. The caller must mask it in the GIC.
    Unhandled,
}

/// An interrupt service routine. It runs with IRQs masked, on the exception stack.
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

/// Put `id` in event mode: interrupts are counted instead of handled by a function.
/// Returns `false` if `id` is out of range. Does not enable the interrupt in the GIC.
pub fn register_event(id: u32) -> bool {
    match HANDLERS.get(id as usize) {
        Some(slot) => {
            slot.store(EVENT, Ordering::Release);
            true
        }
        None => false,
    }
}

/// If `id` has fired since the last call, the total number of times it has fired.
pub fn take_event(id: u32) -> Option<u32> {
    let (e, c) = (EVENTS.get(id as usize)?, CONSUMED.get(id as usize)?);
    let n = e.load(Ordering::Acquire);
    (n != c.load(Ordering::Relaxed)).then(|| {
        c.store(n, Ordering::Relaxed);
        n
    })
}

/// Handle interrupt `id`: run its handler, or count it if it is in event mode. The
/// caller (the vector entry) masks the interrupt in the GIC for [`Dispatched::Event`] and
/// [`Dispatched::Unhandled`].
pub fn dispatch(id: u32) -> Dispatched {
    let p = HANDLERS
        .get(id as usize)
        .map_or(0, |s| s.load(Ordering::Acquire));
    match p {
        0 => {
            UNHANDLED.fetch_add(1, Ordering::Relaxed);
            Dispatched::Unhandled
        }
        EVENT => {
            EVENTS[id as usize].fetch_add(1, Ordering::Release);
            Dispatched::Event
        }
        p => {
            // SAFETY: the only other values stored are `Handler` function pointers cast
            // to `usize` by `register`.
            let f: Handler = unsafe { core::mem::transmute::<usize, Handler>(p) };
            f();
            Dispatched::Handler
        }
    }
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
        assert_eq!(dispatch(143), Dispatched::Unhandled);
        assert!(unhandled_count() >= 1);
        assert!(register(143, h));
        assert_eq!(dispatch(143), Dispatched::Handler);
        assert_eq!(HITS.load(Ordering::SeqCst), 1);
        unregister(143);
        assert_eq!(dispatch(143), Dispatched::Unhandled);
        assert!(!register(192, h));
        assert_eq!(dispatch(1000), Dispatched::Unhandled);
    }

    #[test]
    fn event_mode_counts_and_reports_each_new_event_once() {
        assert!(register_event(71));
        assert_eq!(take_event(71), None);
        assert_eq!(dispatch(71), Dispatched::Event);
        assert_eq!(dispatch(71), Dispatched::Event);
        assert_eq!(take_event(71), Some(2)); // cumulative, like a UIO read
        assert_eq!(take_event(71), None); // nothing new
        assert_eq!(dispatch(71), Dispatched::Event);
        assert_eq!(take_event(71), Some(3));
        assert!(!register_event(500));
        assert_eq!(take_event(500), None);
    }
}
