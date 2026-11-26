//! RFSoC 4x2 base overlay on the real-time kernel: a mood light on the two colour LEDs.
//!
//! Five tasks, each with its own deadline; a higher number is more urgent:
//!
//! | task | priority | what it does |
//! |---|---|---|
//! | `buttons` | 6 | sleeps until the interrupt controller interrupts, then acts on the buttons and switches |
//! | `pwm` | 5 | every millisecond, dims the colour LEDs by switching them on for part of eight steps (125 Hz) |
//! | `selftest` | 4 | raises both GPIOs' interrupts from software, times the way to `buttons`, prints the verdict, ends |
//! | `colour` | 3 | every 20 ms, moves the colour |
//! | `report` | 2 | every five seconds, prints the state and how late `pwm` has ever been |
//!
//! Button 0 changes the pattern (rainbow, breathing white, the colour of switches 0 to 2), 1 and 2 the brightness,
//! 3 switches the lights off and on. The white LEDs show the pattern's number.
//!
//! The tasks share the generated `Design` behind a lock with priority inheritance, held only for a few register
//! accesses; the settings are a few atomic numbers.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use core::time::Duration;
use design_1::rt::{
    IrqEvent, Periodic, Semaphore, Shared, WAIT_FOREVER, delay_ms, spawn, start, task_stack,
};
use design_1::{Design, halt, pac, periph_hz, psu};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::IrqLine;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{Mmio, PhysMem};
use psu::println;

psu::entry!(main);

/// An AXI GPIO's interrupt status register: writing 1 to a bit toggles it, which raises the interrupt from software.
const GPIO_STATUS: usize = 0x120;

/// Brightness steps of the pulse-width modulation.
const STEPS: u32 = 8;
const PATTERNS: u32 = 3;

static PATTERN: AtomicU32 = AtomicU32::new(0);
static BRIGHTNESS: AtomicU32 = AtomicU32::new(STEPS); // 1..=STEPS
static OFF: AtomicBool = AtomicBool::new(false);
static SWITCH_STATE: AtomicU32 = AtomicU32::new(0);
/// How bright each of the six colour channels is, 0..=STEPS.
static LEVEL: [AtomicU32; 6] = [const { AtomicU32::new(0) }; 6];
static PRESSES: AtomicU32 = AtomicU32::new(0);
static PWM_RUNS: AtomicU32 = AtomicU32::new(0);
static PWM_WORST_US: AtomicU32 = AtomicU32::new(0);
/// How often `buttons` has woken, and the semaphore it gives each time.
static WAKES: AtomicU32 = AtomicU32::new(0);
static WOKE: Semaphore = Semaphore::new(0, 8);

static DESIGN: Shared<Design<PhysMem>> = Shared::new();

/// The bit of the interrupt controller that `line` feeds, as the design wires it.
fn input_of(line: &IrqLine) -> u32 {
    let (_, input) = line.via_intc.expect("behind the interrupt controller");
    1 << input
}

fn buttons() {
    let both =
        input_of(&pac::btns_gpio::IRQ_IP2INTC_IRPT) | input_of(&pac::sws_gpio::IRQ_IP2INTC_IRPT);
    let event = IrqEvent::for_line(&pac::axi_intc_0::IRQ_IRQ).expect("the controller's interrupt");
    let mut last = {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.leds_gpio.set_input_mask(Channel::One, 0).unwrap();
        d.axi_intc_0.init().unwrap();
        d.axi_intc_0.enable(both).unwrap();
        d.btns_gpio.enable_interrupt(Channel::One).unwrap();
        d.sws_gpio.enable_interrupt(Channel::One).unwrap();
        SWITCH_STATE.store(d.sws_gpio.read(Channel::One).unwrap(), Relaxed);
        d.btns_gpio.read(Channel::One).unwrap() & 0xF
    };
    loop {
        // Unmask the interrupt, then sleep until it fires: the processor is the other tasks' meanwhile.
        event.rearm();
        event.wait(WAIT_FOREVER);
        WAKES.fetch_add(1, Relaxed);
        WOKE.give();
        delay_ms(20); // let the contacts settle
        let state = {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            let pending = d.axi_intc_0.pending().unwrap();
            // Clear the sources first, then the controller.
            d.btns_gpio.clear_interrupt(Channel::One).unwrap();
            d.sws_gpio.clear_interrupt(Channel::One).unwrap();
            d.axi_intc_0.ack(pending).unwrap();
            SWITCH_STATE.store(d.sws_gpio.read(Channel::One).unwrap() & 0xF, Relaxed);
            d.btns_gpio.read(Channel::One).unwrap() & 0xF
        };
        let pressed = state & !last;
        last = state;
        if pressed == 0 {
            continue;
        }
        PRESSES.fetch_add(1, Relaxed);
        if pressed & 0b0001 != 0 {
            PATTERN.store((PATTERN.load(Relaxed) + 1) % PATTERNS, Relaxed);
        }
        let brightness = BRIGHTNESS.load(Relaxed);
        if pressed & 0b0010 != 0 {
            BRIGHTNESS.store((brightness + 1).min(STEPS), Relaxed);
        }
        if pressed & 0b0100 != 0 {
            BRIGHTNESS.store(brightness.saturating_sub(1).max(1), Relaxed);
        }
        if pressed & 0b1000 != 0 {
            OFF.fetch_xor(true, Relaxed);
        }
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.leds_gpio
            .write(Channel::One, PATTERN.load(Relaxed))
            .unwrap();
    }
}

fn pwm() {
    DESIGN
        .lock(WAIT_FOREVER)
        .unwrap()
        .rgbleds_gpio
        .set_input_mask(Channel::One, 0)
        .unwrap();
    let mut every = Periodic::every_ms(1);
    let mut last = Instant::now();
    let mut step = 0;
    loop {
        every.wait_next();
        // How far this run is from one millisecond after the last.
        let now = Instant::now();
        let gap = now.duration_since(last).as_micros() as u32;
        last = now;
        if PWM_RUNS.fetch_add(1, Relaxed) > 10 {
            PWM_WORST_US.fetch_max(gap.abs_diff(1000), Relaxed);
        }
        let mut bits = 0;
        for (i, level) in LEVEL.iter().enumerate() {
            if level.load(Relaxed) > step {
                bits |= 1 << i;
            }
        }
        step = (step + 1) % STEPS;
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.rgbleds_gpio.write(Channel::One, bits).unwrap();
    }
}

/// A triangle wave over 0..768: up for a third, down for a third, dark for a third.
fn wave(phase: u32) -> u32 {
    match phase % 768 {
        p @ 0..256 => p,
        p @ 256..512 => 511 - p,
        _ => 0,
    }
}

fn colour() {
    let mut every = Periodic::every_ms(20);
    let mut phase = 0u32;
    loop {
        every.wait_next();
        phase = (phase + 3) % 768;
        // Red, green and blue, 0 to 255.
        let rgb = match PATTERN.load(Relaxed) {
            0 => [wave(phase), wave(phase + 256), wave(phase + 512)],
            1 => {
                let up_down = phase.min(767 - phase) * 255 / 383;
                [up_down; 3]
            }
            _ => {
                let switches = SWITCH_STATE.load(Relaxed);
                core::array::from_fn(|i| if switches & (1 << i) != 0 { 255 } else { 0 })
            }
        };
        let brightness = if OFF.load(Relaxed) {
            0
        } else {
            BRIGHTNESS.load(Relaxed)
        };
        let [r, g, b] = rgb.map(|v| (v * brightness).div_ceil(255));
        // The overlay wires each of the two LEDs as blue, green, red.
        for (level, value) in LEVEL.iter().zip([b, g, r, b, g, r]) {
            level.store(value, Relaxed);
        }
    }
}

fn selftest() {
    // Let the others set themselves up.
    delay_ms(200);
    while WOKE.take(0) {}
    let mut good = true;
    for name in ["buttons", "switches"] {
        let before = WAKES.load(Relaxed);
        let raised = {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            let gpio = if name == "buttons" {
                &d.btns_gpio
            } else {
                &d.sws_gpio
            };
            let raised = Instant::now();
            gpio.regs().write32(GPIO_STATUS, 1).unwrap();
            raised
        };
        let woke = WOKE.take(100);
        let took = raised.elapsed();
        println!(
            "{name} interrupt woke the buttons task: {woke}, in {} us",
            took.as_micros()
        );
        // The task is the most urgent one: it runs almost at once.
        good &= woke && took < Duration::from_millis(1);
        delay_ms(100);
        good &= WAKES.load(Relaxed) == before + 1;
    }
    delay_ms(2000);
    let (runs, worst) = (PWM_RUNS.load(Relaxed), PWM_WORST_US.load(Relaxed));
    println!("pwm task: {runs} runs, worst {worst} us from its millisecond");
    good &= runs > 2000 && worst < 100;
    // Something to see: the colour channels have been moving.
    good &= LEVEL.iter().any(|l| l.load(Relaxed) > 0);
    println!(
        "{}",
        if good {
            "RT LIGHTS OK"
        } else {
            "RT LIGHTS FAILED"
        }
    );
    println!("\nbutton 0 pattern, 1 brighter, 2 dimmer, 3 off and on");
}

fn report() {
    loop {
        delay_ms(5000);
        println!(
            "pattern {}, brightness {}/{STEPS}, switches {:04b}, {} presses, pwm worst {} us",
            PATTERN.load(Relaxed),
            BRIGHTNESS.load(Relaxed),
            SWITCH_STATE.load(Relaxed),
            PRESSES.load(Relaxed),
            PWM_WORST_US.load(Relaxed)
        );
    }
}

fn main() -> ! {
    // Checks that the FPGA is configured, sets up the processor and maps every block of the design.
    let design = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== RFSoC 4x2 base overlay: a mood light on the kernel ==");
    DESIGN.set(design);
    spawn("buttons", 6, task_stack!(8192).unwrap(), buttons).unwrap();
    spawn("pwm", 5, task_stack!(8192).unwrap(), pwm).unwrap();
    spawn("selftest", 4, task_stack!(8192).unwrap(), selftest).unwrap();
    spawn("colour", 3, task_stack!(8192).unwrap(), colour).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(periph_hz().unwrap(), 1000)
}
