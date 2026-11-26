//! RFSoC 4x2 base overlay, bare metal: the buttons and switches act through an interrupt.
//!
//! The buttons' and switches' GPIOs raise an interrupt when a pin changes; it goes through the AXI interrupt
//! controller to the processor. The program first checks that path by itself and prints `SELF TEST OK`, then for a
//! minute: button 0 counts up on the LEDs, button 1 counts down, button 2 clears, button 3 lights both colour LEDs in
//! the colour switches 0 to 2 choose (red, green, blue).
#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, pac, psu};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::IrqLine;
use hardroxide_mmio::Mmio;
use hardroxide_mmio::time::{Instant, sleep};
use psu::println;
use psu::wait::IrqWait;

psu::entry!(main);

/// An AXI GPIO's interrupt status register: writing 1 to a bit toggles it, which raises the interrupt from software.
const GPIO_STATUS: usize = 0x120;
/// The system monitor's latest temperature reading.
const TEMPERATURE: usize = 0x400;

/// The bit of the interrupt controller that `line` feeds, as the design wires it.
fn input_of(line: &IrqLine) -> u32 {
    let (_, input) = line.via_intc.expect("behind the interrupt controller");
    1 << input
}

/// The colour LEDs' six bits for both LEDs showing the colour `rgb` (bit 0 red, 1 green, 2 blue): the overlay wires
/// each LED as blue, green, red.
fn colour_bits(rgb: u32) -> u32 {
    let bgr = (rgb & 1) << 2 | (rgb & 2) | (rgb & 4) >> 2;
    bgr | bgr << 3
}

/// Raise each GPIO's interrupt from software and follow it to the processor.
fn self_test(d: &Design<hardroxide_mmio::PhysMem>, irq: &IrqWait) -> bool {
    let mut good = true;
    for (leds, mask) in [(&d.leds_gpio, 0xF), (&d.rgbleds_gpio, 0x3F)] {
        for pattern in [0x15 & mask, 0x2A & mask, mask, 0] {
            leds.write(Channel::One, pattern).unwrap();
            good &= leds.read(Channel::One).unwrap() & mask == pattern;
            sleep(Duration::from_millis(150));
        }
    }
    println!("LEDs follow what is written: {good}");
    let controller = &d.axi_intc_0;
    for (name, gpio, input) in [
        (
            "buttons",
            &d.btns_gpio,
            input_of(&pac::btns_gpio::IRQ_IP2INTC_IRPT),
        ),
        (
            "switches",
            &d.sws_gpio,
            input_of(&pac::sws_gpio::IRQ_IP2INTC_IRPT),
        ),
    ] {
        gpio.clear_interrupt(Channel::One).unwrap();
        controller.ack(input).unwrap();
        let quiet = controller.pending().unwrap() & input == 0;
        gpio.regs().write32(GPIO_STATUS, 1).unwrap();
        let arrived = controller
            .wait(irq, input, Some(Duration::from_millis(100)))
            .unwrap()
            .is_some();
        gpio.clear_interrupt(Channel::One).unwrap();
        controller.ack(input).unwrap();
        let cleared = controller.pending().unwrap() & input == 0;
        println!("{name} interrupt: quiet before {quiet}, arrived {arrived}, cleared {cleared}");
        good &= quiet && arrived && cleared;
    }
    good
}

fn main() -> ! {
    // Checks that the FPGA is configured, sets up the processor and maps every block of the design.
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== RFSoC 4x2 base overlay ==");
    let reading = d.system_management_wiz_0.read32(TEMPERATURE).unwrap() as u16;
    println!(
        "FPGA temperature: {} C",
        psu::sysmon::millicelsius(reading) / 1000
    );

    let (buttons, switches, leds, colour, controller) = (
        &d.btns_gpio,
        &d.sws_gpio,
        &d.leds_gpio,
        &d.rgbleds_gpio,
        &d.axi_intc_0,
    );
    let button_input = input_of(&pac::btns_gpio::IRQ_IP2INTC_IRPT);
    let switch_input = input_of(&pac::sws_gpio::IRQ_IP2INTC_IRPT);
    // The one line from the controller to the processor.
    let irq = IrqWait::for_line(&pac::axi_intc_0::IRQ_IRQ).expect("the controller's interrupt");
    leds.set_input_mask(Channel::One, 0).unwrap();
    colour.set_input_mask(Channel::One, 0).unwrap();
    controller.init().unwrap();
    controller.enable(button_input | switch_input).unwrap();
    buttons.enable_interrupt(Channel::One).unwrap();
    switches.enable_interrupt(Channel::One).unwrap();
    d.enable_interrupts();

    if self_test(&d, &irq) {
        println!("SELF TEST OK");
    } else {
        println!("SELF TEST FAILED");
    }

    println!("\nnow press the buttons and move the switches (60 s)");
    let mut count = 0u32;
    let mut last = buttons.read(Channel::One).unwrap() & 0xF;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        let waited = controller.wait(
            &irq,
            button_input | switch_input,
            Some(Duration::from_millis(500)),
        );
        let Some(pending) = waited.unwrap() else {
            continue;
        };
        sleep(Duration::from_millis(20)); // contacts bounce for a few milliseconds
        // Clear the sources first, then the controller.
        buttons.clear_interrupt(Channel::One).unwrap();
        switches.clear_interrupt(Channel::One).unwrap();
        controller.ack(pending).unwrap();
        let state = buttons.read(Channel::One).unwrap() & 0xF;
        let setting = switches.read(Channel::One).unwrap() & 0xF;
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
            colour.write(Channel::One, colour_bits(setting)).unwrap();
        }
        leds.write(Channel::One, count).unwrap();
        println!("buttons {state:04b}, switches {setting:04b}: count {count}");
    }
    println!("done");
    halt()
}
