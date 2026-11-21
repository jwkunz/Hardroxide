//! A 10 ms control loop under the kernel: the control task copies the buttons to the LEDs
//! every 10 ms (priority 5); a lower-priority task reports the loop's timing every second.
//! Both share the design through a `Shared`. Tutorial 13.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{Periodic, Shared, WAIT_FOREVER, delay_ms, spawn, start, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::{PhysMem, time::Instant};
use ps7::println;

ps7::entry!(main);

static DESIGN: Shared<Design<PhysMem>> = Shared::new();
static WORST_US: AtomicU32 = AtomicU32::new(0);
static RUNS: AtomicU32 = AtomicU32::new(0);

fn control() {
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.axi_gpio_0.set_input_mask(Channel::One, 0).unwrap();
    }
    let mut every = Periodic::every_ms(10);
    let mut last = Instant::now();
    loop {
        every.wait_next();
        let now = Instant::now();
        let period = now.duration_since(last).as_micros() as u32;
        last = now;
        if RUNS.fetch_add(1, Relaxed) > 0 {
            WORST_US.fetch_max(period.abs_diff(10_000), Relaxed);
        }
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        let buttons = d.axi_gpio_0.read(Channel::Two).unwrap() & 0xF;
        d.axi_gpio_0.write(Channel::One, buttons).unwrap();
    }
}

fn report() {
    loop {
        delay_ms(1000);
        println!(
            "control: {} runs, worst deviation from the 10 ms period {} us",
            RUNS.load(Relaxed),
            WORST_US.load(Relaxed)
        );
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
    println!("\n== control: buttons -> LEDs every 10 ms ==");
    DESIGN.set(design);
    spawn("control", 5, task_stack!(8192).unwrap(), control).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
