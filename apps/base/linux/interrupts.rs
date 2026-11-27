//! PYNQ-Z2 base overlay under Linux: the buttons act through an interrupt.
//!
//! The buttons' GPIO raises an interrupt when a pin changes; it goes through the AXI interrupt controller
//! (`system_interrupts`) to the processor, where Linux hands it to the program as a UIO device (apply the crate's
//! `interrupts.dtso` first). The program checks that path by itself and prints `SELF TEST OK`, then for half a minute
//! (or the number of seconds given as the first argument) it sleeps until a button is pressed: button 0 counts up on
//! the LEDs, button 1 counts down, button 2 clears, button 3 ends the program. Run as root. Linux tutorial 2.
use design_1::{Design, pac};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::{DevMem, Mmio};
use std::error::Error;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// An AXI GPIO's interrupt status register: writing 1 to a bit toggles it, which raises the interrupt from software.
const GPIO_STATUS: usize = 0x120;

/// Raise the buttons' interrupt from software and follow it to the program.
fn self_test(d: &Design<DevMem>, irq: &impl IrqWaiter, input: u32) -> Result<bool, Box<dyn Error>> {
    let (buttons, controller) = (&d.btns_gpio, &d.system_interrupts);
    buttons.clear_interrupt(Channel::One)?;
    controller.ack(input)?;
    let quiet = controller.pending()? & input == 0;
    buttons.regs().write32(GPIO_STATUS, 1)?;
    let arrived = controller
        .wait(irq, input, Some(Duration::from_millis(100)))?
        .is_some();
    buttons.clear_interrupt(Channel::One)?;
    controller.ack(input)?;
    let cleared = controller.pending()? & input == 0;
    println!("buttons interrupt: quiet before {quiet}, arrived {arrived}, cleared {cleared}");
    Ok(quiet && arrived && cleared)
}

fn main() -> Result<(), Box<dyn Error>> {
    let seconds = match std::env::args().nth(1) {
        Some(s) => s.parse()?,
        None => 30,
    };
    let d = Design::load()?;
    let (buttons, leds, controller) = (&d.btns_gpio, &d.leds_gpio, &d.system_interrupts);
    // Which input of the controller the buttons feed, from the design.
    let (_, number) = pac::btns_gpio::IRQ_IP2INTC_IRPT
        .via_intc
        .expect("the buttons' line goes through the controller");
    let input = 1 << number;
    println!("the buttons' interrupt is input {number} of the controller");
    // The one line from the controller to the processor.
    let irq = pac::system_interrupts::IRQ_IRQ.open()?;

    leds.set_input_mask(Channel::One, 0)?;
    controller.init()?;
    controller.enable(input)?;
    buttons.enable_interrupt(Channel::One)?;
    if self_test(&d, &irq, input)? {
        println!("SELF TEST OK");
    } else {
        println!("SELF TEST FAILED");
    }

    println!("\nnow press the buttons ({seconds} s; button 3 ends)");
    let mut count = 0u32;
    let mut last = buttons.read(Channel::One)? & 0xF;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        // The program sleeps here, using no processor time, until the interrupt or the time-out.
        if controller
            .wait(&irq, input, Some(Duration::from_millis(500)))?
            .is_none()
        {
            continue;
        }
        sleep(Duration::from_millis(20)); // contacts bounce for a few milliseconds
        buttons.clear_interrupt(Channel::One)?; // clear the source first...
        controller.ack(input)?; // ...then the controller
        let state = buttons.read(Channel::One)? & 0xF;
        let pressed = state & !last; // buttons that went down
        last = state;
        if pressed == 0 {
            continue;
        }
        if pressed & 0b1000 != 0 {
            break;
        }
        if pressed & 0b0001 != 0 {
            count = (count + 1) & 0xF;
        }
        if pressed & 0b0010 != 0 {
            count = count.wrapping_sub(1) & 0xF;
        }
        if pressed & 0b0100 != 0 {
            count = 0;
        }
        leds.write(Channel::One, count)?;
        println!("buttons {pressed:04b} pressed: count {count}");
    }
    // Leave the hardware quiet.
    controller.disable(input)?;
    leds.write(Channel::One, 0)?;
    println!("done");
    Ok(())
}
