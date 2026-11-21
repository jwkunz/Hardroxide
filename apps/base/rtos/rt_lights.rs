//! Base overlay, RTOS: a mood light. Four tasks, each with its own deadline:
//!
//! * `buttons` (priority 6) sleeps on the interrupt controller's interrupt (`IrqEvent`) and wakes
//!   within microseconds of a press: button 0 changes the pattern, 1 and 2 the brightness,
//!   3 switches the lights off and on;
//! * `pwm` (priority 5) runs every millisecond and drives the two RGB LEDs with 8-step pulse-width
//!   modulation (125 Hz), so a colour can be dimmed although the GPIO is on/off only;
//! * `colour` (priority 3) moves the colour every 20 ms;
//! * `report` (priority 2) prints the press count and the worst lateness of the PWM task.
//!
//! Base overlay application 3.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use design_1::rt::{IrqEvent, Periodic, Shared, WAIT_FOREVER, delay_ms, spawn, start, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::{PhysMem, time::Instant};
use ps7::println;

ps7::entry!(main);

static DESIGN: Shared<Design<PhysMem>> = Shared::new();
const BTNS_BIT: u32 = 1 << 13;
const STEPS: u32 = 8;

/// 0 rainbow, 1 breathing white, 2 red, 3 green.
static MODE: AtomicU32 = AtomicU32::new(0);
static BRIGHT: AtomicU32 = AtomicU32::new(STEPS); // 1..=STEPS
static OFF: AtomicBool = AtomicBool::new(false);
/// Current level of each of the six channels, 0..=STEPS.
static LEVEL: [AtomicU32; 6] = [const { AtomicU32::new(0) }; 6];
static PRESSES: AtomicU32 = AtomicU32::new(0);
static PWM_WORST_US: AtomicU32 = AtomicU32::new(0);

fn buttons() {
    let ev = IrqEvent::for_line(&design_1::pac::system_interrupts::IRQ_IRQ).unwrap();
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.system_interrupts.init().unwrap();
        d.system_interrupts.enable(BTNS_BIT).unwrap();
        d.btns_gpio.enable_interrupt(Channel::One).unwrap();
        d.leds_gpio.set_input_mask(Channel::One, 0).unwrap();
    }
    ev.rearm();
    let mut last = 0;
    loop {
        ev.wait(WAIT_FOREVER);
        delay_ms(20); // let the contacts settle
        let state = {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            let state = d.btns_gpio.read(Channel::One).unwrap() & 0xF;
            d.btns_gpio.clear_interrupt(Channel::One).unwrap();
            d.system_interrupts.ack(BTNS_BIT).unwrap();
            state
        };
        ev.rearm();
        let pressed = state & !last;
        last = state;
        if pressed == 0 {
            continue;
        }
        PRESSES.fetch_add(1, Relaxed);
        if pressed & 1 != 0 {
            MODE.store((MODE.load(Relaxed) + 1) % 4, Relaxed);
        }
        if pressed & 2 != 0 {
            BRIGHT.store((BRIGHT.load(Relaxed) + 1).min(STEPS), Relaxed);
        }
        if pressed & 4 != 0 {
            BRIGHT.store(BRIGHT.load(Relaxed).saturating_sub(1).max(1), Relaxed);
        }
        if pressed & 8 != 0 {
            OFF.fetch_xor(true, Relaxed);
        }
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.leds_gpio.write(Channel::One, MODE.load(Relaxed)).unwrap();
    }
}

fn pwm() {
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.rgbleds_gpio.set_input_mask(Channel::One, 0).unwrap();
    }
    let mut every = Periodic::every_ms(1);
    let mut last = Instant::now();
    let (mut step, mut runs) = (0u32, 0u32);
    loop {
        every.wait_next();
        let now = Instant::now();
        let gap = now.duration_since(last).as_micros() as u32;
        last = now;
        runs += 1;
        if runs > 10 {
            PWM_WORST_US.fetch_max(gap.abs_diff(1000), Relaxed);
        }
        let mut bits = 0u32;
        for (i, l) in LEVEL.iter().enumerate() {
            if l.load(Relaxed) > step {
                bits |= 1 << i;
            }
        }
        step = (step + 1) % STEPS;
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.rgbleds_gpio.write(Channel::One, bits).unwrap();
    }
}

fn colour() {
    let mut every = Periodic::every_ms(20);
    let mut phase = 0u32; // 0..768
    loop {
        every.wait_next();
        phase = (phase + 3) % 768;
        let (r, g, b) = match MODE.load(Relaxed) {
            0 => {
                // rainbow: a triangle wave per channel, 120 degrees apart
                let tri = |p: u32| {
                    let p = p % 768;
                    if p < 256 { p } else if p < 512 { 511 - p } else { 0 }
                };
                (tri(phase), tri(phase + 256), tri(phase + 512))
            }
            1 => {
                let t = if phase < 384 { phase } else { 767 - phase };
                (t * 255 / 384, t * 255 / 384, t * 255 / 384)
            }
            2 => (255, 0, 0),
            _ => (0, 255, 0),
        };
        let bright = BRIGHT.load(Relaxed);
        let scale = |v: u32| if OFF.load(Relaxed) { 0 } else { (v * bright * STEPS / 255).div_ceil(STEPS).min(STEPS) };
        let (r, g, b) = (scale(r), scale(g), scale(b));
        for (i, v) in [r, g, b, r, g, b].into_iter().enumerate() {
            LEVEL[i].store(v, Relaxed);
        }
    }
}

fn report() {
    loop {
        delay_ms(5000);
        println!(
            "mood light: mode {}, brightness {}/{}, {} button presses, pwm task worst lateness {} us",
            MODE.load(Relaxed),
            BRIGHT.load(Relaxed),
            STEPS,
            PRESSES.load(Relaxed),
            PWM_WORST_US.load(Relaxed)
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
    println!("\n== mood light: button 0 pattern, 1/2 brightness, 3 off/on ==");
    DESIGN.set(design);
    spawn("buttons", 6, task_stack!(8192).unwrap(), buttons).unwrap();
    spawn("pwm", 5, task_stack!(8192).unwrap(), pwm).unwrap();
    spawn("colour", 3, task_stack!(8192).unwrap(), colour).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
