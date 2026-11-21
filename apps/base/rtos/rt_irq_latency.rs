//! Base overlay, RTOS: how long does a hardware interrupt take to reach a task?
//!
//! A low-priority task raises an input of the interrupt controller by software (its simulation mode), which
//! raises the controller's output exactly like a real PL interrupt would; it travels through the GIC
//! and the kernel's interrupt path to a priority-6 task sleeping on an `IrqEvent`. The time from the
//! write to the task running is recorded with the 3 ns global timer, 2000 times, with a
//! priority-1 task hogging the processor and a priority-2 task thrashing the cache meanwhile.
//! Prints a histogram and `LATENCY OK` if the worst case is under 50 us. Base overlay application 4.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use design_1::rt::{IrqEvent, Shared, WAIT_FOREVER, delay_ms, spawn, start, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_mmio::PhysMem;
use ps7::println;

ps7::entry!(main);

static DESIGN: Shared<Design<PhysMem>> = Shared::new();
const SOFT: u32 = 1 << 12; // an input nothing in the design drives
const ROUNDS: u32 = 2000;
/// Bucket upper edges in microseconds.
const EDGES: [u64; 8] = [5, 8, 12, 20, 30, 50, 100, 1000];
static HIST: [AtomicU32; 9] = [const { AtomicU32::new(0) }; 9];
static WORST_NS: AtomicU64 = AtomicU64::new(0);
static SUM_NS: AtomicU64 = AtomicU64::new(0);
static T0_NS: AtomicU64 = AtomicU64::new(0);

use hardroxide_ps7::time::now_ns;

fn waiter() {
    let ev = IrqEvent::for_line(&design_1::pac::system_interrupts::IRQ_IRQ).unwrap();
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.system_interrupts.start_simulation().unwrap();
        d.system_interrupts.enable(SOFT).unwrap();
    }
    ev.rearm();
    let mut done = 0;
    while done < ROUNDS {
        if !ev.wait(500) {
            println!("timeout waiting for the interrupt");
            continue;
        }
        let t1 = now_ns();
        let ns = t1.saturating_sub(T0_NS.load(Relaxed));
        {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            d.system_interrupts.ack(SOFT).unwrap();
        }
        ev.rearm();
        let us = ns / 1000;
        let b = EDGES.iter().position(|&e| us < e).unwrap_or(EDGES.len());
        HIST[b].fetch_add(1, Relaxed);
        WORST_NS.fetch_max(ns, Relaxed);
        SUM_NS.fetch_add(ns, Relaxed);
        done += 1;
    }
    println!("{ROUNDS} interrupts, interrupt controller to task:");
    println!("  mean {} ns, worst {} ns", SUM_NS.load(Relaxed) / u64::from(ROUNDS), WORST_NS.load(Relaxed));
    let mut lo = 0;
    for (i, h) in HIST.iter().enumerate() {
        let hi = EDGES.get(i).copied();
        match hi {
            Some(hi) => println!("  {lo:>4} to {hi:>4} us: {}", h.load(Relaxed)),
            None => println!("  {lo:>4} us and over: {}", h.load(Relaxed)),
        }
        lo = hi.unwrap_or(lo);
    }
    if WORST_NS.load(Relaxed) < 50_000 {
        println!("LATENCY OK");
    } else {
        println!("LATENCY TOO HIGH");
    }
    loop {
        delay_ms(1000);
    }
}

fn trigger() {
    delay_ms(100);
    for _ in 0..ROUNDS {
        {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            T0_NS.store(now_ns(), Relaxed);
            d.system_interrupts.simulate(SOFT).unwrap();
        }
        delay_ms(3);
    }
    loop {
        delay_ms(1000);
    }
}

static mut BUF: [u8; 1 << 20] = [0; 1 << 20];

/// Load on the system: a task that never sleeps and one that walks a megabyte.
fn hog() {
    loop {
        core::hint::spin_loop();
    }
}
fn thrash() {
    let mut i = 0usize;
    loop {
        // SAFETY: only this task touches `BUF`.
        unsafe {
            let p = core::ptr::addr_of_mut!(BUF) as *mut u8;
            for k in 0..4096 {
                let j = (i + k * 64) % (1 << 20);
                *p.add(j) = (*p.add(j)).wrapping_add(1);
            }
        }
        i = (i + 4096 * 64) % (1 << 20);
        delay_ms(1);
    }
}

fn main() -> ! {
    let design = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== interrupt-to-task latency under load ==");
    DESIGN.set(design);
    spawn("waiter", 6, task_stack!(8192).unwrap(), waiter).unwrap();
    spawn("trigger", 4, task_stack!(8192).unwrap(), trigger).unwrap();
    spawn("thrash", 2, task_stack!(8192).unwrap(), thrash).unwrap();
    spawn("hog", 1, task_stack!(8192).unwrap(), hog).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
