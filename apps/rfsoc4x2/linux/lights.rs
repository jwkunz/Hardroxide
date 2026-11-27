//! RFSoC 4x2 base overlay under Linux: the buttons and switches act through an interrupt.
//!
//! The buttons' and switches' GPIOs raise an interrupt when a pin changes; it goes through the AXI interrupt
//! controller to the processor, where Linux hands it to the program as a UIO device. The program first checks that
//! path by itself and prints `SELF TEST OK`, then for a minute (or the number of seconds given as its argument):
//! button 0 counts up on the LEDs, button 1 counts down, button 2 clears, button 3 lights both colour LEDs in the
//! colour switches 0 to 2 choose (red, green, blue). Run as root.
use design_1::{Design, pac};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::{IrqLine, IrqWaiter};
use hardroxide_mmio::{DevMem, Mmio};
use std::error::Error;
use std::thread::sleep;
use std::time::{Duration, Instant};

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

/// Raise each GPIO's interrupt from software and follow it to the program.
fn self_test(d: &Design<DevMem>, irq: &impl IrqWaiter) -> Result<bool, Box<dyn Error>> {
    let mut good = true;
    for (leds, mask) in [(&d.leds_gpio, 0xF), (&d.rgbleds_gpio, 0x3F)] {
        for pattern in [0x15 & mask, 0x2A & mask, mask, 0] {
            leds.write(Channel::One, pattern)?;
            good &= leds.read(Channel::One)? & mask == pattern;
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
        gpio.clear_interrupt(Channel::One)?;
        controller.ack(input)?;
        let quiet = controller.pending()? & input == 0;
        gpio.regs().write32(GPIO_STATUS, 1)?;
        let arrived = controller
            .wait(irq, input, Some(Duration::from_millis(100)))?
            .is_some();
        gpio.clear_interrupt(Channel::One)?;
        controller.ack(input)?;
        let cleared = controller.pending()? & input == 0;
        println!("{name} interrupt: quiet before {quiet}, arrived {arrived}, cleared {cleared}");
        good &= quiet && arrived && cleared;
    }
    Ok(good)
}

fn main() -> Result<(), Box<dyn Error>> {
    let seconds = match std::env::args().nth(1) {
        Some(s) => s.parse()?,
        None => 60,
    };
    // Loads the bitstream, sets the clocks and maps every block of the design.
    let d = Design::load()?;
    println!("== RFSoC 4x2 base overlay under Linux ==");
    let reading = d.system_management_wiz_0.read32(TEMPERATURE)? as u16;
    // This family's system monitor: its reading's transfer function.
    let celsius = f64::from(reading) * 509.314 / 65536.0 - 280.2309;
    println!("FPGA temperature: {celsius:.0} C");

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
    let irq = pac::axi_intc_0::IRQ_IRQ.open()?;
    leds.set_input_mask(Channel::One, 0)?;
    colour.set_input_mask(Channel::One, 0)?;
    controller.init()?;
    controller.enable(button_input | switch_input)?;
    buttons.enable_interrupt(Channel::One)?;
    switches.enable_interrupt(Channel::One)?;

    if self_test(&d, &irq)? {
        println!("SELF TEST OK");
    } else {
        println!("SELF TEST FAILED");
    }

    println!("\nnow press the buttons and move the switches ({seconds} s)");
    let mut count = 0u32;
    let mut last = buttons.read(Channel::One)? & 0xF;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        // The program sleeps here, using no processor time, until the interrupt or the time-out.
        let waited = controller.wait(
            &irq,
            button_input | switch_input,
            Some(Duration::from_millis(500)),
        )?;
        let Some(pending) = waited else {
            continue;
        };
        sleep(Duration::from_millis(20)); // contacts bounce for a few milliseconds
        // Clear the sources first, then the controller.
        buttons.clear_interrupt(Channel::One)?;
        switches.clear_interrupt(Channel::One)?;
        controller.ack(pending)?;
        let state = buttons.read(Channel::One)? & 0xF;
        let setting = switches.read(Channel::One)? & 0xF;
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
            colour.write(Channel::One, colour_bits(setting))?;
        }
        leds.write(Channel::One, count)?;
        println!("buttons {state:04b}, switches {setting:04b}: count {count}");
    }
    // Leave the hardware quiet.
    controller.disable(button_input | switch_input)?;
    leds.write(Channel::One, 0)?;
    colour.write(Channel::One, 0)?;
    println!("done");
    Ok(())
}
