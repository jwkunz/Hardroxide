//! Jitter benchmark for hardroxide-rt on a Zynq-7000 (run for hours on the board).
//!
//! Measures, under load, with the global timer (3 ns resolution):
//!   * release lateness of a 10 ms periodic task (priority 5): how far after its ideal release
//!     (a fixed grid) the task actually runs;
//!   * interrupt-to-task latency: from raising a software interrupt to the waiting
//!     priority-7 task running (entry, dispatch, semaphore give, context switch).
//!
//! Load: a priority-1 task that never sleeps, a priority-3 task that thrashes a 4 MiB array
//! (beyond the 512 KiB L2) for about 3 ms of every 10 ms, and a priority-2 task that prints the
//! report over the UART. Cumulative histograms are printed every minute.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};
use hardroxide_ps7::gic::Gic;
use hardroxide_ps7::{clocks, console, irq, map, println, time};
use hardroxide_rt::{Periodic, Semaphore, WAIT_FOREVER, delay_ms, spawn, start, task_stack, ticks};

hardroxide_ps7::entry!(main);

const PS_CLK_HZ: u64 = 50_000_000;
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
static SGI_SENT_NS: AtomicU64 = AtomicU64::new(0);
static SGI_SEM: Semaphore = Semaphore::new(0, 1);

fn time_ns() -> u64 {
    time::now_ns()
}

fn platform_init() {
    // SAFETY: this program owns the SLCR for the duration of this setup.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let w = |o: usize, v: u32| slcr.write32(o, v).unwrap();
    let r = |o: usize| slcr.read32(o).unwrap();
    w(0x008, 0xDF0D);
    w(0x12C, r(0x12C) | (1 << 20));
    let io_fdiv = (r(0x108) >> 12) & 0x7F;
    w(0x154, ((io_fdiv / 2).max(1) << 8) | 0x3);
    w(0x228, 0);
    w(0x738, 0x16E1);
    w(0x73C, 0x16E0);
    w(0x004, 0x767B);
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
    // SAFETY: the distributor window is used here only to raise one software interrupt.
    let be = unsafe { PhysMem::new() };
    let gic = Gic::new(
        be.map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE)
            .unwrap(),
        be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE).unwrap(),
    );
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
        RELEASE.print("10 ms release lateness");
        ISR.print("interrupt-to-task latency");
    }
}

fn main() -> ! {
    platform_init();
    // SAFETY: early in start-up, running from DDR at its physical address.
    unsafe { hardroxide_ps7::mem::enable(512 << 20) };
    // SAFETY: SLCR is only read here; the console UART is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let uart_ref = clocks::uart_ref_hz(&slcr, PS_CLK_HZ).unwrap();
    // SAFETY: as above.
    unsafe { console::init(map::UART0_BASE, uart_ref as u32, 115_200) };
    let periph = clocks::periph_hz(&slcr, PS_CLK_HZ).unwrap();
    println!("\n== hardroxide-rt jitter benchmark: reports every minute ==");
    // SAFETY: nothing else touches the global timer.
    unsafe { time::init(periph) };
    let gic = Gic::new(
        be.map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE)
            .unwrap(),
        be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE).unwrap(),
    );
    gic.init().unwrap();
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
    start(periph, 1000)
}
