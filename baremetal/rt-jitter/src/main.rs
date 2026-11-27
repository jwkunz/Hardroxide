//! Jitter benchmark for hardroxide-rt on a Zynq-7000 or a Zynq UltraScale+ (the target chooses; run for hours on the
//! board).
//!
//! Measures, under load, with the time source (the Zynq-7000's global timer, 3 ns resolution; the UltraScale+ system
//! counter, 30 ns):
//!   * release lateness of a 10 ms periodic task (priority 5): how far after its ideal release
//!     (a fixed grid) the task actually runs;
//!   * interrupt-to-task latency: from raising a software interrupt to the waiting
//!     priority-7 task running (entry, dispatch, semaphore give, context switch).
//!
//! Load: a priority-1 task that never sleeps, a priority-3 task that thrashes a 4 MiB array
//! (beyond the L2 cache: 512 KiB on the Zynq-7000, 1 MiB on UltraScale+) for about 3 ms of every 10 ms, and a priority-2 task that prints the
//! report over the UART. Cumulative histograms are printed every minute.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use hardroxide_rt::{Periodic, Semaphore, WAIT_FOREVER, delay_ms, spawn, start, task_stack, ticks};
use platform::ps::{self, irq, println, time};

#[path = "../../common/platform.rs"]
mod platform;

ps::entry!(main);

/// Bucket upper edges in microseconds; the last bucket is everything above.
const EDGES: [u64; 12] = [1, 2, 3, 5, 8, 12, 20, 50, 100, 500, 2000, 10_000];
const N: usize = EDGES.len() + 1;

struct Hist {
    b: [AtomicU32; N],
    count: AtomicU64,
    sum_ns: AtomicU64,
    max_ns: AtomicU64,
}

impl Hist {
    const fn new() -> Self {
        Hist {
            b: [const { AtomicU32::new(0) }; N],
            count: AtomicU64::new(0),
            sum_ns: AtomicU64::new(0),
            max_ns: AtomicU64::new(0),
        }
    }
    fn add(&self, ns: u64) {
        let us = ns / 1000;
        let i = EDGES.iter().position(|&e| us < e).unwrap_or(N - 1);
        self.b[i].fetch_add(1, Relaxed);
        self.count.fetch_add(1, Relaxed);
        self.sum_ns.fetch_add(ns, Relaxed);
        self.max_ns.fetch_max(ns, Relaxed);
    }
    fn print(&self, name: &str) {
        let n = self.count.load(Relaxed).max(1);
        println!(
            "  {name}: n={} mean={} ns max={} ns",
            self.count.load(Relaxed),
            self.sum_ns.load(Relaxed) / n,
            self.max_ns.load(Relaxed)
        );
        for i in 0..N {
            let c = self.b[i].load(Relaxed);
            if c == 0 {
                continue;
            }
            if i < N - 1 {
                println!("    < {:>5} us : {c}", EDGES[i]);
            } else {
                println!("    >= {:>4} us : {c}", EDGES[N - 2]);
            }
        }
    }
}

static RELEASE: Hist = Hist::new();
static ISR: Hist = Hist::new();
static MISSED: AtomicU32 = AtomicU32::new(0);
/// The furthest a release came before the grid's best estimate: more than the clock's resolution and the baseline's own
/// uncertainty would mean the tick and the time source disagree, and the lateness figures would be wrong.
static EARLIEST_NS: AtomicU64 = AtomicU64::new(0);
static SGI_SENT_NS: AtomicU64 = AtomicU64::new(0);
static SGI_SEM: Semaphore = Semaphore::new(0, 1);

fn time_ns() -> u64 {
    time::now_ns()
}

// ---- the 10 ms task under test ----
fn control() {
    let mut p = Periodic::every_ms(10);
    p.wait_next(); // align to the grid
    let t0 = time_ns();
    let mut k = 0u64;
    let mut base_offset = i64::MAX;
    loop {
        if !p.wait_next() {
            MISSED.fetch_add(1, Relaxed);
        }
        k += 1;
        let off = time_ns() as i64 - (t0 + k * 10_000_000) as i64;
        // The tick phase relative to the first release is unknown by up to one tick; the
        // smallest offset seen is the best estimate of "no lateness" and later samples are
        // measured against it.
        if k <= 100 {
            base_offset = base_offset.min(off);
            continue;
        }
        RELEASE.add((off - base_offset).max(0) as u64);
        EARLIEST_NS.fetch_max((base_offset - off).max(0) as u64, Relaxed);
    }
}

// ---- interrupt to task ----
fn sgi_handler() {
    SGI_SEM.give_from_isr();
}

fn isr_waiter() {
    loop {
        SGI_SEM.take(WAIT_FOREVER);
        let t = time_ns();
        ISR.add(t.saturating_sub(SGI_SENT_NS.load(Relaxed)));
    }
}

fn isr_trigger() {
    let gic = platform::gic();
    let mut i = 0u32;
    loop {
        // A period that is not a multiple of the tick, so the interrupt lands at all phases.
        delay_ms(7 + (i % 3));
        i += 1;
        SGI_SENT_NS.store(time_ns(), Relaxed);
        gic.send_sgi(1, 1).unwrap();
    }
}

// ---- load ----
fn hog() {
    let mut x = 0u32;
    loop {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        core::hint::black_box(x);
    }
}

fn thrash() {
    static mut BIG: [u32; 1 << 20] = [0; 1 << 20]; // 4 MiB
    let mut p = Periodic::every_ms(10);
    loop {
        let t = time_ns();
        // SAFETY: only this task touches BIG.
        let big = unsafe { &mut *core::ptr::addr_of_mut!(BIG) };
        let mut i = 0usize;
        while time_ns() - t < 3_000_000 {
            for _ in 0..256 {
                big[i] = big[i].wrapping_add(1);
                i = (i + 4099) & ((1 << 20) - 1);
            }
        }
        p.wait_next();
    }
}

fn report() {
    let mut minutes = 0u32;
    loop {
        delay_ms(if cfg!(feature = "fast") {
            10_000
        } else {
            60_000
        });
        minutes += 1;
        println!("\n== after {minutes} min (kernel ticks {}) ==", ticks());
        println!("  missed releases: {}", MISSED.load(Relaxed));
        println!(
            "  earliest release before the baseline: {} ns",
            EARLIEST_NS.load(Relaxed)
        );
        RELEASE.print("10 ms release lateness");
        ISR.print("interrupt-to-task latency");
    }
}

fn main() -> ! {
    let board = platform::init("hardroxide-rt jitter benchmark: reports every minute");
    let gic = &board.gic;
    assert!(irq::register(1, sgi_handler));
    gic.set_priority(1, 0x40).unwrap();
    gic.enable(1).unwrap();

    macro_rules! task {
        ($name:literal, $prio:expr, $f:expr) => {
            spawn($name, $prio, task_stack!(8192).unwrap(), $f).unwrap();
        };
    }
    task!("isr_waiter", 7, isr_waiter);
    task!("isr_trigger", 6, isr_trigger);
    task!("control", 5, control);
    if !cfg!(feature = "no-thrash") {
        task!("thrash", 3, thrash);
    }
    task!("report", 2, report);
    if !cfg!(feature = "no-hog") {
        task!("hog", 1, hog);
    }
    start(board.tick_clock_hz, 1000)
}
