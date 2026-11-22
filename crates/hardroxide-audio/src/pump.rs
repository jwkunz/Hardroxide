//! An interrupt-driven audio pump: a lock-free ring of frames filled by a task and emptied by a periodic interrupt that moves one frame to the
//! codec controller per sample.
//!
//! The controller holds a single frame and has no FIFO, so a frame must be written within one sample period (20.5 us). Polling that from a
//! task is not possible beside other work; instead a timer interrupt at (about) twice the sample rate calls [`Pump::service`], which writes the next frame
//! when the controller says a sample tick has come. A task keeps the ring full with [`Pump::push`]. When the ring is empty the pump plays silence and counts an
//! underrun. Everything here is atomic, so the interrupt and the task need no lock.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use hardroxide_ip::audio::{AudioCodecCtrl, Frame};
use hardroxide_mmio::Mmio;

/// A ring of `N` frames (`N` a power of two), a producer task and a consumer interrupt.
pub struct Pump<const N: usize> {
    left: [AtomicU32; N],
    right: [AtomicU32; N],
    /// Next slot the interrupt reads (only the interrupt writes it).
    head: AtomicUsize,
    /// Next slot the task writes (only the task writes it).
    tail: AtomicUsize,
    serviced: AtomicU32,
    underruns: AtomicU32,
    muted: AtomicU32,
}

impl<const N: usize> Default for Pump<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Pump<N> {
    pub const fn new() -> Self {
        assert!(N.is_power_of_two() && N >= 2);
        Pump {
            left: [const { AtomicU32::new(0) }; N],
            right: [const { AtomicU32::new(0) }; N],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            serviced: AtomicU32::new(0),
            underruns: AtomicU32::new(0),
            muted: AtomicU32::new(0),
        }
    }

    /// Frames waiting in the ring.
    pub fn len(&self) -> usize {
        self.tail
            .load(Ordering::Acquire)
            .wrapping_sub(self.head.load(Ordering::Acquire))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Free slots.
    pub fn space(&self) -> usize {
        N - 1 - self.len()
    }

    /// Queue a frame (the producer task only); `false` if the ring is full.
    pub fn push(&self, f: Frame) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(self.head.load(Ordering::Acquire)) >= N - 1 {
            return false;
        }
        self.left[tail % N].store(f.left as u32, Ordering::Relaxed);
        self.right[tail % N].store(f.right as u32, Ordering::Relaxed);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    /// Take the oldest frame (the consumer interrupt only).
    fn pop(&self) -> Option<Frame> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        let f = Frame {
            left: self.left[head % N].load(Ordering::Relaxed) as i32,
            right: self.right[head % N].load(Ordering::Relaxed) as i32,
        };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(f)
    }

    /// Discard everything queued (the producer task only, while the pump is stopped or between pushes: the frames in flight are lost).
    pub fn flush(&self) {
        self.head
            .store(self.tail.load(Ordering::Acquire), Ordering::Release);
    }

    /// Play silence instead of the queued frames (the ring still drains, so the audio stays in step).
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(u32::from(muted), Ordering::Relaxed);
    }

    /// The timer interrupt's work: if a sample tick has come, clear it and write the next frame (silence if the ring is empty).
    /// Returns whether a frame was written. Call it at least twice per sample period.
    pub fn service<R: Mmio>(&self, ctrl: &AudioCodecCtrl<R>) -> bool {
        if !ctrl.ready().unwrap_or(false) {
            return false;
        }
        let _ = ctrl.clear_ready();
        let f = match self.pop() {
            Some(f) if self.muted.load(Ordering::Relaxed) == 0 => f,
            Some(_) => Frame::default(),
            None => {
                self.underruns.fetch_add(1, Ordering::Relaxed);
                Frame::default()
            }
        };
        let _ = ctrl.write_frame(f);
        self.serviced.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Frames written to the controller so far.
    pub fn serviced(&self) -> u32 {
        self.serviced.load(Ordering::Relaxed)
    }

    /// Sample ticks at which the ring was empty (silence was played).
    pub fn underruns(&self) -> u32 {
        self.underruns.load(Ordering::Relaxed)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const BASE: u64 = 0x43C0_0000;

    fn ctrl() -> (MockBackend, AudioCodecCtrl<hardroxide_mmio::MockRegion>) {
        let be = MockBackend::new();
        let c = AudioCodecCtrl::new(be.map(BASE, 0x1000).unwrap());
        (be, c)
    }

    #[test]
    fn frames_come_out_in_order_and_the_ring_reports_full_and_empty() {
        let p: Pump<8> = Pump::new();
        assert!(p.is_empty());
        assert_eq!(p.space(), 7);
        for i in 0..7 {
            assert!(p.push(Frame { left: i, right: -i }));
        }
        assert!(!p.push(Frame::default()), "full: one slot is kept free");
        assert_eq!(p.len(), 7);
        let (be, c) = ctrl();
        for i in 0..7 {
            // A sample tick arrives; the interrupt writes the next frame.
            be.poke(BASE + 0x10, 1);
            assert!(p.service(&c));
            assert_eq!(
                (be.peek(BASE + 0x08), be.peek(BASE + 0x0C)),
                (i as u32 & 0xFF_FFFF, (-i) as u32 & 0xFF_FFFF)
            );
        }
        assert_eq!(p.serviced(), 7);
        assert_eq!(p.underruns(), 0);
        // The ring wraps: many more frames than slots go through.
        for k in 0..100 {
            assert!(p.push(Frame {
                left: 1000 + k,
                right: 0
            }));
            be.poke(BASE + 0x10, 1);
            assert!(p.service(&c));
            assert_eq!(be.peek(BASE + 0x08), (1000 + k) as u32);
        }
    }

    #[test]
    fn no_tick_means_no_write_and_an_empty_ring_plays_silence_and_counts_an_underrun() {
        let p: Pump<4> = Pump::new();
        let (be, c) = ctrl();
        assert!(
            !p.service(&c),
            "the controller has no new sample: nothing to do"
        );
        assert_eq!(p.serviced(), 0);
        be.poke(BASE + 0x08, 0x123456);
        be.poke(BASE + 0x10, 1);
        assert!(p.service(&c));
        assert_eq!(be.peek(BASE + 0x08), 0, "silence");
        assert_eq!((p.serviced(), p.underruns()), (1, 1));
    }

    #[test]
    fn muting_plays_silence_but_keeps_draining() {
        let p: Pump<4> = Pump::new();
        let (be, c) = ctrl();
        p.push(Frame { left: 5, right: 5 });
        p.push(Frame { left: 6, right: 6 });
        p.set_muted(true);
        be.poke(BASE + 0x10, 1);
        p.service(&c);
        assert_eq!(be.peek(BASE + 0x08), 0);
        assert_eq!(p.len(), 1, "the frame was consumed");
        p.set_muted(false);
        be.poke(BASE + 0x10, 1);
        p.service(&c);
        assert_eq!(be.peek(BASE + 0x08), 6);
        assert_eq!(p.underruns(), 0);
    }

    #[test]
    fn a_producer_thread_and_an_interrupt_thread_lose_and_reorder_nothing() {
        use std::sync::Arc;
        let p: Arc<Pump<64>> = Arc::new(Pump::new());
        let be = MockBackend::new();
        let regs = be.map(BASE, 0x1000).unwrap();
        let c = AudioCodecCtrl::new(regs);
        let producer = {
            let p = p.clone();
            std::thread::spawn(move || {
                let mut i = 0;
                while i < 20_000 {
                    if p.push(Frame { left: i, right: i }) {
                        i += 1;
                    } else {
                        std::thread::yield_now();
                    }
                }
            })
        };
        // The "interrupt": a sample tick arrives, the pump writes a frame; frames that were really queued must come out in order,
        // and a tick that finds the ring empty plays silence and consumes nothing.
        let mut next = 0u32;
        while next < 20_000 {
            let before = p.underruns();
            be.poke(BASE + 0x10, 1);
            assert!(p.service(&c));
            if p.underruns() == before {
                assert_eq!(be.peek(BASE + 0x08), next, "frame {next}");
                next += 1;
            }
        }
        producer.join().unwrap();
    }
}
