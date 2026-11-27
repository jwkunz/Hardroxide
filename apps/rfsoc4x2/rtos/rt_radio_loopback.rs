//! RFSoC 4x2 base overlay on the real-time kernel: the radio loopback as one task, beside a 1 ms control loop that it
//! must not disturb.
//!
//! | task | priority | what it does |
//! |---|---|---|
//! | `control` | 5 | every millisecond, notes how late it is and blinks the processor's first LED |
//! | `radio` | 3 | sends the tones, records them and computes their spectra, then reports both |
//!
//! SMA cables from DAC A to ADC A and from DAC B to ADC B. The program programs the board's clock chips before the
//! kernel starts, prints one line per check and the control loop's worst deviation, then `RT RADIO LOOPBACK OK`.
//!
//! Beside the generated crate's own, it needs `hardroxide-board` as a development dependency, the clock chips'
//! register files (`fixtures/rfsoc4x2/clocks/*.txt`) next to this file, and `common/loopback.rs` and
//! `common/clocks.rs` in `common/`.
#![no_std]
#![no_main]

use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{Periodic, Shared, WAIT_FOREVER, spawn, start, task_stack};
use design_1::{Design, halt, periph_hz, psu};
use hardroxide_board::rfsoc4x2 as board;
use hardroxide_dma::static_dma_buffer;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem};
use psu::gpio::Gpio;
use psu::println;

#[path = "common/clocks.rs"]
mod clocks;
#[path = "common/loopback.rs"]
mod loopback;
use loopback::{BUFFER_BYTES, SAMPLES, Spectrum};

psu::entry!(main);

/// The control loop's worst deviation from its period that still passes, in microseconds.
const ALLOWED_US: u32 = 20;

static DESIGN: Shared<Design<PhysMem>> = Shared::new();
static RELEASES: AtomicU32 = AtomicU32::new(0);
static WORST_US: AtomicU32 = AtomicU32::new(0);

/// The spectrum's working memory: too large for a stack.
static mut RE: [f64; SAMPLES] = [0.0; SAMPLES];
static mut IM: [f64; SAMPLES] = [0.0; SAMPLES];

fn control() {
    // SAFETY: the pin used here is this task's alone; the clocks' pins were set before the kernel started.
    let be = unsafe { PhysMem::new() };
    let gpio = Gpio::new(
        be.map(psu::map::GPIO_BASE as u64, psu::map::GPIO_SIZE)
            .unwrap(),
    );
    let led = board::PS_LEDS[0];
    gpio.set_output(led, false).unwrap();
    let mut every = Periodic::every_ms(1);
    let mut last = Instant::now();
    loop {
        every.wait_next();
        let now = Instant::now();
        let period_us = now.duration_since(last).as_micros() as u32;
        last = now;
        let release = RELEASES.fetch_add(1, Relaxed);
        if release > 0 {
            WORST_US.fetch_max(period_us.abs_diff(1000), Relaxed);
        }
        gpio.write(led, release / 250 % 2 == 1).unwrap();
    }
}

fn radio() {
    let buffers = (
        static_dma_buffer!(BUFFER_BYTES).unwrap(),
        static_dma_buffer!(BUFFER_BYTES).unwrap(),
    );
    // SAFETY: this task runs once, and these are the only references ever made to either array.
    let (re, im) = unsafe { (&mut *addr_of_mut!(RE), &mut *addr_of_mut!(IM)) };
    let mut print = |text: core::fmt::Arguments| psu::print!("{text}");
    // Count the control loop's releases while the radio works, and only its deviations from then.
    let before = RELEASES.load(Relaxed);
    WORST_US.store(0, Relaxed);
    let failures = {
        let mut design = DESIGN.lock(WAIT_FOREVER).unwrap();
        let d = &mut *design;
        loopback::measure(loopback::base_overlay_bench!(
            d,
            buffers,
            Spectrum::new(re, im),
            &mut print
        ))
    };
    let releases = RELEASES.load(Relaxed) - before;
    let worst = WORST_US.load(Relaxed);
    println!(
        "control loop: {releases} releases while the radio worked, worst deviation from 1 ms: {worst} us"
    );
    if failures == 0 && releases > 100 && worst <= ALLOWED_US {
        println!("RT RADIO LOOPBACK OK");
    } else {
        println!("RT RADIO LOOPBACK FAILED: {failures} radio check(s)");
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
    println!("\n== RFSoC 4x2 base overlay: radio loopback beside a 1 ms control loop ==");
    if !clocks::program() {
        println!("RT RADIO LOOPBACK FAILED: the clock chips did not lock");
        halt()
    }
    DESIGN.set(design);
    spawn("control", 5, task_stack!(8192).unwrap(), control).unwrap();
    spawn("radio", 3, task_stack!(65536).unwrap(), radio).unwrap();
    start(periph_hz().unwrap(), 1000)
}
