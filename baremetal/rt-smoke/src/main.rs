//! Smoke test for hardroxide-rt on a Zynq-7000 or a Zynq UltraScale+ (the target chooses): priority preemption, periodic
//! release, semaphores, queues, interrupt-to-task hand-off, priority inheritance, an interrupt before the kernel starts,
//! registers across task switches, and the report of a fault in a task. Runs under QEMU (`--features qemu`, see
//! `scripts/test-qemu.sh`) and on the boards over JTAG.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};
use hardroxide_rt::{
    Mutex, Periodic, Queue, Semaphore, WAIT_FOREVER, delay_ms, spawn, start, task_stack, ticks,
};
use platform::ps::{self, cpu, irq, println};

#[path = "../../common/platform.rs"]
mod platform;
mod registers;

ps::entry!(main);

const SCENARIOS: u32 = 7;

static DONE: Semaphore = Semaphore::new(0, 16);
static QUEUE: Queue<u32, 4> = Queue::new();
static ISR_SEM: Semaphore = Semaphore::new(0, 1);
static LOCK: Mutex = Mutex::new();
static HOLDING: Semaphore = Semaphore::new(0, 1);
static COUNTER: AtomicU32 = AtomicU32::new(0);
static STOP_SPIN: AtomicU32 = AtomicU32::new(0);
static PI_ACQUIRED_AT: AtomicU64 = AtomicU64::new(0);
static PI_START: AtomicU64 = AtomicU64::new(0);

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
    send_sgi(1);
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

// ---- scenario 6: a task sleeps on a hardware interrupt (IrqEvent, software interrupt 2) ----
fn event_waiter() {
    delay_ms(1000);
    let ev = hardroxide_rt::IrqEvent::new(2).unwrap();
    ev.rearm();
    let t0 = ticks();
    let first = ev.wait(200);
    let first_after = ticks() - t0;
    ev.rearm();
    let second = ev.wait(200);
    let none = ev.wait(30);
    println!(
        "[6] irq event: first {first} after {first_after} ticks, second (after rearm) {second}, nothing pending {}",
        !none
    );
    DONE.give();
}
fn event_trigger() {
    delay_ms(1040);
    send_sgi(2);
    delay_ms(60);
    send_sgi(2);
}
fn send_sgi(n: u32) {
    platform::gic().send_sgi(n, 1).unwrap();
}

// ---- scenario 7: two equally urgent tasks load every register they can with their own values, and check them after the
// other has run (the tick switches between them while they spin) ----
const ROUNDS: u32 = 100;
static SWITCHES: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static BROKEN: AtomicU32 = AtomicU32::new(0);
static REGISTER_TASKS_DONE: AtomicU32 = AtomicU32::new(0);

fn register_task(me: usize) {
    delay_ms(1200);
    for round in 0..ROUNDS {
        SWITCHES[me].fetch_add(1, SeqCst);
        let seed = (u64::from(round) << 8) | (me as u64 + 1);
        if !registers::survive_a_switch(seed, &SWITCHES[1 - me]) {
            BROKEN.fetch_add(1, SeqCst);
        }
    }
    if REGISTER_TASKS_DONE.fetch_add(1, SeqCst) == 1 {
        let broken = BROKEN.load(SeqCst);
        println!(
            "[7] registers: {} checks in two tasks switching under them, {broken} broken ({})",
            2 * ROUNDS,
            if broken == 0 {
                "preserved"
            } else {
                "CORRUPTED"
            }
        );
        DONE.give();
    }
    // Keep the other task's wait ending until it is done too.
    while REGISTER_TASKS_DONE.load(SeqCst) < 2 {
        SWITCHES[me].fetch_add(1, SeqCst);
        delay_ms(1);
    }
}
fn registers_a() {
    register_task(0);
}
fn registers_b() {
    register_task(1);
}

fn coordinator() {
    COORDINATOR_RAN.store(1, SeqCst);
    for _ in 0..SCENARIOS {
        DONE.take(WAIT_FOREVER);
    }
    println!("RT ALL OK");
    println!("-- executing an undefined instruction in a task on purpose");
    registers::undefined_instruction();
    println!("not reached");
    loop {
        delay_ms(1000);
    }
}

// ---- before the kernel starts: an interrupt is handled and `main` goes on ----
static EARLY: AtomicU32 = AtomicU32::new(0);
/// The most urgent task sets this first thing: it must still be clear when `main` calls `start`.
static COORDINATOR_RAN: AtomicU32 = AtomicU32::new(0);
static BEFORE_START: Semaphore = Semaphore::new(0, 1);
fn early_handler() {
    EARLY.fetch_add(1, SeqCst);
}

fn main() -> ! {
    let board = platform::init("hardroxide-rt smoke test");
    let gic = &board.gic;
    assert!(irq::register(1, sgi_handler));
    gic.set_priority(1, 0x40).unwrap();
    gic.enable(1).unwrap();
    gic.set_priority(2, 0x40).unwrap();

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
    task!("event_waiter", 4, event_waiter);
    task!("event_trigger", 6, event_trigger);
    task!("registers_a", 2, registers_a);
    task!("registers_b", 2, registers_b);

    assert!(irq::register(3, early_handler));
    gic.set_priority(3, 0x40).unwrap();
    gic.enable(3).unwrap();
    // SAFETY: the handler and the controller are set up.
    unsafe { cpu::enable_irq() };
    send_sgi(3);
    while EARLY.load(SeqCst) == 0 {
        core::hint::spin_loop();
    }
    cpu::disable_irq();
    // Kernel calls from `main` do not switch to a task either, and one that would have to wait fails.
    BEFORE_START.give();
    let took = BEFORE_START.take(0);
    let waited = BEFORE_START.take(5);
    delay_ms(1);
    println!("[0] before start: give and take {took}, a take that would wait {waited}");
    println!(
        "[0] before start: an interrupt was handled, main went on and {}",
        if COORDINATOR_RAN.load(SeqCst) == 0 {
            "no task ran"
        } else {
            "A TASK RAN EARLY"
        }
    );
    start(board.tick_clock_hz, 1000)
}
