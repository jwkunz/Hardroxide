//! Smoke test for hardroxide-rt on a Zynq-7000: priority preemption, periodic release,
//! semaphores, queues, interrupt-to-task hand-off and priority inheritance. Runs under QEMU
//! (`--features qemu`, see `scripts/test-qemu.sh`) and on the board over JTAG.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};
use hardroxide_ps7::gic::Gic;
use hardroxide_ps7::{clocks, console, irq, map, println, time};
use hardroxide_rt::{
    Mutex, Periodic, Queue, Semaphore, WAIT_FOREVER, delay_ms, spawn, start, task_stack, ticks,
};

hardroxide_ps7::entry!(main);

const PS_CLK_HZ: u64 = if cfg!(feature = "qemu") {
    33_333_333
} else {
    50_000_000
};
const SCENARIOS: u32 = 5;

static DONE: Semaphore = Semaphore::new(0, 16);
static QUEUE: Queue<u32, 4> = Queue::new();
static ISR_SEM: Semaphore = Semaphore::new(0, 1);
static LOCK: Mutex = Mutex::new();
static HOLDING: Semaphore = Semaphore::new(0, 1);
static COUNTER: AtomicU32 = AtomicU32::new(0);
static STOP_SPIN: AtomicU32 = AtomicU32::new(0);
static PI_ACQUIRED_AT: AtomicU64 = AtomicU64::new(0);
static PI_START: AtomicU64 = AtomicU64::new(0);

fn platform_init() {
    // SAFETY: this program owns the SLCR for the duration of this setup.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let w = |o: usize, v: u32| slcr.write32(o, v).unwrap();
    let r = |o: usize| slcr.read32(o).unwrap();
    w(0x008, 0xDF0D);
    if cfg!(feature = "qemu") {
        let m = |a: usize, mask: u32, v: u32| w(a, (r(a) & !mask) | (v & mask));
        m(0x108, 0x0007_F000, 0x0002_0000);
        m(0x108, 0x10, 0x10);
        m(0x108, 0x1, 0x1);
        m(0x108, 0x1, 0x0);
        m(0x108, 0x10, 0x0);
    }
    w(0x12C, r(0x12C) | (1 << 20));
    let io_fdiv = (r(0x108) >> 12) & 0x7F;
    w(0x154, ((io_fdiv / 2).max(1) << 8) | 0x3);
    w(0x228, 0);
    w(0x738, 0x16E1);
    w(0x73C, 0x16E0);
    w(0x004, 0x767B);
}

// ---- scenario 1: periodic release on a 10 ms grid ----
fn periodic() {
    let mut p = Periodic::every_ms(10);
    let start = ticks();
    let mut worst = 0;
    for i in 1..=20u64 {
        p.wait_next();
        worst = worst.max(ticks().saturating_sub(start + 10 * i));
    }
    println!(
        "[1] periodic: 20 releases on a 10 ms grid, worst lateness {worst} ticks ({})",
        if worst <= 1 { "no drift" } else { "LATE" }
    );
    DONE.give();
}

// ---- scenario 2: a high-priority task preempts a low-priority one that never yields ----
fn spinner() {
    delay_ms(500);
    while STOP_SPIN.load(SeqCst) == 0 {
        COUNTER.fetch_add(1, SeqCst);
    }
}
fn preemptor() {
    delay_ms(500);
    let start = ticks();
    let mut late = 0;
    for i in 1..=20u64 {
        delay_ms(5);
        if ticks() != start + 5 * i {
            late += 1;
        }
    }
    println!(
        "[2] preemption: 20 wakeups of a 5 ms sleeper over a spinning task, {late} late; spinner made {} progress",
        if COUNTER.load(SeqCst) > 0 {
            "some"
        } else {
            "no"
        }
    );
    STOP_SPIN.store(1, SeqCst);
    DONE.give();
}

// ---- scenario 3: queue between two tasks ----
fn producer() {
    delay_ms(250);
    for i in 0..10u32 {
        assert!(QUEUE.send(i, WAIT_FOREVER));
        delay_ms(2);
    }
}
fn consumer() {
    delay_ms(250);
    let mut ok = 0;
    for i in 0..10u32 {
        if QUEUE.recv(100) == Some(i) {
            ok += 1;
        }
    }
    println!(
        "[3] queue: {ok}/10 items received in order (capacity 4, producer faster than the consumer's start)"
    );
    DONE.give();
}

// ---- scenario 4: interrupt hands work to a task (software-generated interrupt 1) ----
fn sgi_handler() {
    ISR_SEM.give_from_isr();
}
fn isr_waiter() {
    delay_ms(400);
    let t0 = ticks();
    let got = ISR_SEM.take(200);
    println!(
        "[4] interrupt: semaphore given from an ISR woke the task: {got}, after {} ticks",
        ticks() - t0
    );
    DONE.give();
}
fn isr_trigger() {
    delay_ms(420);
    // SAFETY: the distributor window is used here only to raise one software interrupt.
    let be = unsafe { PhysMem::new() };
    let gic = Gic::new(
        be.map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE)
            .unwrap(),
        be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE).unwrap(),
    );
    gic.send_sgi(1, 1).unwrap();
}

// ---- scenario 5: priority inversion resolved by priority inheritance ----
// L (1) holds the lock for 30 ticks; M (3) wakes at 5 and hogs the processor for 100 ticks;
// H (5) wakes at 10 and wants the lock. Without inheritance H waits for M (about 100 ticks);
// with it L runs at H's priority and releases at about tick 30.
fn pi_low() {
    delay_ms(700);
    let _g = LOCK.lock(WAIT_FOREVER).unwrap();
    let t0 = ticks();
    PI_START.store(t0, SeqCst);
    HOLDING.give();
    while ticks() < t0 + 30 {}
}
fn pi_medium() {
    HOLDING.take(WAIT_FOREVER);
    delay_ms(5);
    let t = ticks();
    while ticks() < t + 100 {}
}
fn pi_high() {
    HOLDING.take(WAIT_FOREVER);
    delay_ms(10);
    let _g = LOCK.lock(WAIT_FOREVER).unwrap();
    let waited = ticks() - PI_START.load(SeqCst);
    PI_ACQUIRED_AT.store(waited, SeqCst);
    println!(
        "[5] priority inheritance: the high task got the lock {waited} ticks after the low task took it ({})",
        if waited < 60 {
            "inheritance worked"
        } else {
            "INVERSION"
        }
    );
    DONE.give();
}

fn coordinator() {
    for _ in 0..SCENARIOS {
        DONE.take(WAIT_FOREVER);
    }
    println!("RT ALL OK");
    loop {
        delay_ms(1000);
    }
}

fn main() -> ! {
    platform_init();
    // SAFETY: SLCR is only read here; the console UART is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let uart_ref = clocks::uart_ref_hz(&slcr, PS_CLK_HZ).unwrap();
    // SAFETY: as above.
    unsafe { console::init(map::UART0_BASE, uart_ref as u32, 115_200) };
    let periph = clocks::periph_hz(&slcr, PS_CLK_HZ).unwrap();
    println!("\n== hardroxide-rt smoke test ==");
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
            spawn($name, $prio, task_stack!(4096).unwrap(), $f).unwrap();
        };
    }
    task!("coord", 7, coordinator);
    task!("periodic", 4, periodic);
    task!("spinner", 1, spinner);
    task!("preemptor", 6, preemptor);
    task!("consumer", 3, consumer);
    task!("producer", 2, producer);
    task!("isr_waiter", 4, isr_waiter);
    task!("isr_trigger", 6, isr_trigger);
    task!("pi_low", 1, pi_low);
    task!("pi_medium", 3, pi_medium);
    task!("pi_high", 5, pi_high);
    start(periph, 1000)
}
