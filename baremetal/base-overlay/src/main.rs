//! The RFSoC 4x2's base overlay on bare metal: the buttons and switches act through an interrupt.
//!
//! The base overlay is the FPGA design that RFSoC-PYNQ ships (`fixtures/board/rfsoc4x2_base_overlay.xsa`). Here the
//! buttons' and switches' GPIOs raise an interrupt when a pin changes; it goes through the AXI interrupt controller to
//! the processor's interrupt controller. The program first checks that path by itself and prints `SELF TEST OK`, then
//! for a minute: button 0 counts up on the LEDs, button 1 counts down, button 2 clears, button 3 lights both colour
//! LEDs in the colour switches 0 to 2 choose (red, green, blue).
//!
//! The addresses below are the overlay's. A generated crate will name them for you once the generator supports this
//! processor family.

#![no_std]
#![no_main]

use core::time::Duration;
use hardroxide_ip::gpio::{AxiGpio, Channel};
use hardroxide_ip::intc::AxiIntc;
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem, PhysRegion};
use platform::ps::{self, cpu, println};
use ps::sysmon;
use ps::wait::IrqWait;

#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;

ps::entry!(main);

// The base overlay's address map.
const INTERRUPT_CONTROLLER: usize = 0x8002_0000;
const BUTTONS: usize = 0x8003_0000;
const LEDS: usize = 0x8004_0000;
const COLOUR_LEDS: usize = 0x8005_0000;
const SWITCHES: usize = 0x8008_0000;
/// The overlay's monitor of the FPGA's own temperature and voltages.
const SYSTEM_MONITOR: usize = 0xB012_0000;
const BLOCK_SIZE: usize = 0x1_0000;
/// Its latest temperature reading.
const TEMPERATURE: usize = 0x400;

// Which inputs of the interrupt controller the two GPIOs feed.
const BUTTONS_INPUT: u32 = 1 << 1;
const SWITCHES_INPUT: u32 = 1 << 2;
/// The controller's output is PL interrupt 0 of the processor.
const PL_INTERRUPT: u32 = 0;

/// An AXI GPIO's interrupt status register; writing 1 to a bit toggles it, which raises the interrupt from software.
const GPIO_STATUS: usize = 0x120;

fn map(base: usize) -> PhysRegion {
    // SAFETY: the overlay's registers are used by this program alone.
    let be = unsafe { PhysMem::new() };
    be.map(base as u64, BLOCK_SIZE).unwrap()
}

/// The colour LEDs' six bits for both LEDs showing the colour `rgb` (bit 0 red, 1 green, 2 blue): the overlay wires
/// each LED as blue, green, red.
fn colour_bits(rgb: u32) -> u32 {
    let bgr = (rgb & 1) << 2 | (rgb & 2) | (rgb & 4) >> 2;
    bgr | bgr << 3
}

struct Board {
    buttons: AxiGpio<PhysRegion>,
    switches: AxiGpio<PhysRegion>,
    leds: AxiGpio<PhysRegion>,
    colour: AxiGpio<PhysRegion>,
    controller: AxiIntc<PhysRegion>,
    irq: IrqWait,
}

/// Raise each GPIO's interrupt from software and follow it to the processor.
fn self_test(b: &Board) -> bool {
    let mut good = true;
    for (gpio, out) in [(&b.leds, 0xF), (&b.colour, 0x3F)] {
        for pattern in [0x15 & out, 0x2A & out, out, 0] {
            gpio.write(Channel::One, pattern).unwrap();
            good &= gpio.read(Channel::One).unwrap() & out == pattern;
            sleep(Duration::from_millis(150));
        }
    }
    println!("LEDs follow what is written: {good}");
    for (name, gpio, base, input) in [
        ("buttons", &b.buttons, BUTTONS, BUTTONS_INPUT),
        ("switches", &b.switches, SWITCHES, SWITCHES_INPUT),
    ] {
        gpio.clear_interrupt(Channel::One).unwrap();
        b.controller.ack(input).unwrap();
        let quiet = b.controller.pending().unwrap() & input == 0;
        map(base).write32(GPIO_STATUS, 1).unwrap();
        let arrived = b
            .controller
            .wait(&b.irq, input, Some(Duration::from_millis(100)))
            .unwrap()
            .is_some();
        gpio.clear_interrupt(Channel::One).unwrap();
        b.controller.ack(input).unwrap();
        let cleared = b.controller.pending().unwrap() & input == 0;
        println!("{name} interrupt: quiet before {quiet}, arrived {arrived}, cleared {cleared}");
        good &= quiet && arrived && cleared;
    }
    good
}

fn main() -> ! {
    platform::init("RFSoC 4x2 base overlay");
    let b = Board {
        buttons: AxiGpio::new(map(BUTTONS)),
        switches: AxiGpio::new(map(SWITCHES)),
        leds: AxiGpio::new(map(LEDS)),
        colour: AxiGpio::new(map(COLOUR_LEDS)),
        controller: AxiIntc::new(map(INTERRUPT_CONTROLLER)),
        irq: IrqWait::new(ps::map::pl_irq(PL_INTERRUPT)).unwrap(),
    };
    b.leds.set_input_mask(Channel::One, 0).unwrap();
    b.colour.set_input_mask(Channel::One, 0).unwrap();
    b.controller.init().unwrap();
    b.controller.enable(BUTTONS_INPUT | SWITCHES_INPUT).unwrap();
    b.buttons.enable_interrupt(Channel::One).unwrap();
    b.switches.enable_interrupt(Channel::One).unwrap();
    // SAFETY: the handlers (the event counting of `IrqWait`) are in place.
    unsafe { cpu::enable_irq() };

    let reading = map(SYSTEM_MONITOR).read32(TEMPERATURE).unwrap() as u16;
    println!(
        "FPGA temperature: {} C",
        sysmon::millicelsius(reading) / 1000
    );

    if self_test(&b) {
        println!("SELF TEST OK");
    } else {
        println!("SELF TEST FAILED");
    }

    println!("\nnow press the buttons and move the switches (60 s)");
    let mut count = 0u32;
    let mut last = b.buttons.read(Channel::One).unwrap() & 0xF;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        let waited = b.controller.wait(
            &b.irq,
            BUTTONS_INPUT | SWITCHES_INPUT,
            Some(Duration::from_millis(500)),
        );
        let Some(pending) = waited.unwrap() else {
            continue;
        };
        sleep(Duration::from_millis(20)); // contacts bounce for a few milliseconds
        // Clear the sources first, then the controller.
        b.buttons.clear_interrupt(Channel::One).unwrap();
        b.switches.clear_interrupt(Channel::One).unwrap();
        b.controller.ack(pending).unwrap();
        let state = b.buttons.read(Channel::One).unwrap() & 0xF;
        let switches = b.switches.read(Channel::One).unwrap() & 0xF;
        let pressed = state & !last; // buttons that went down
        last = state;
        if pressed & 0b0001 != 0 {
            count = (count + 1) & 0xF;
        }
        if pressed & 0b0010 != 0 {
            count = count.wrapping_sub(1) & 0xF;
        }
        if pressed & 0b0100 != 0 {
            count = 0;
        }
        if pressed & 0b1000 != 0 {
            b.colour.write(Channel::One, colour_bits(switches)).unwrap();
        }
        b.leds.write(Channel::One, count).unwrap();
        println!("buttons {state:04b}, switches {switches:04b}: count {count}");
    }
    println!("done");
    loop {
        cpu::wfe();
    }
}
