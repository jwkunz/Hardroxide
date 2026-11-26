//! Base overlay, bare metal: a PL interrupt through the GIC.
//!
//! The buttons' GPIO raises an interrupt when a pin changes; it goes through the AXI interrupt controller
//! (`system_interrupts`) to the processor's interrupt controller (the GIC). The program checks that path by itself and
//! prints `SELF TEST OK`, then for 20 seconds sleeps until a button is pressed: button 0 counts up on the LEDs, button
//! 1 counts down, button 2 clears. Bare metal tutorial 2.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, pac, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::Mmio;
use hardroxide_mmio::time::{Instant, sleep};
use ps7::println;
use ps7::wait::IrqWait;

ps7::entry!(main);

/// An AXI GPIO's interrupt status register: writing 1 to a bit toggles it, which raises the interrupt from software.
const GPIO_STATUS: usize = 0x120;

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let (buttons, leds, controller) = (&d.btns_gpio, &d.leds_gpio, &d.system_interrupts);
    // Which input of the controller the buttons feed, from the design.
    let (_, number) = pac::btns_gpio::IRQ_IP2INTC_IRPT
        .via_intc
        .expect("the buttons' line goes through the controller");
    let input = 1 << number;
    println!("\n== button_irq: the buttons' interrupt is input {number} of the controller ==");
    // The one line from the controller to the processor.
    let irq =
        IrqWait::for_line(&pac::system_interrupts::IRQ_IRQ).expect("controller reaches the PS");

    leds.set_input_mask(Channel::One, 0).unwrap();
    controller.init().unwrap();
    controller.enable(input).unwrap();
    buttons.enable_interrupt(Channel::One).unwrap();
    d.enable_interrupts(); // the GIC and the processor's interrupt mask stay closed until now

    // The path, with no one at the board: raise the interrupt from software.
    buttons.clear_interrupt(Channel::One).unwrap();
    controller.ack(input).unwrap();
    irq.enable().unwrap();
    let quiet = controller.pending().unwrap() & input == 0;
    buttons.regs().write32(GPIO_STATUS, 1).unwrap();
    let arrived = controller
        .wait(&irq, input, Some(Duration::from_millis(100)))
        .unwrap()
        .is_some();
    buttons.clear_interrupt(Channel::One).unwrap();
    controller.ack(input).unwrap();
    let cleared = controller.pending().unwrap() & input == 0;
    println!("buttons interrupt: quiet before {quiet}, arrived {arrived}, cleared {cleared}");
    println!(
        "{}",
        if quiet && arrived && cleared {
            "SELF TEST OK"
        } else {
            "SELF TEST FAILED"
        }
    );

    println!("\nnow press the buttons (20 s)");
    let mut count = 0u32;
    let mut last = buttons.read(Channel::One).unwrap() & 0xF;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        // The processor sleeps here until the interrupt or the time-out.
        if controller
            .wait(&irq, input, Some(Duration::from_millis(500)))
            .unwrap()
            .is_none()
        {
            continue;
        }
        sleep(Duration::from_millis(20)); // contacts bounce for a few milliseconds
        buttons.clear_interrupt(Channel::One).unwrap(); // clear the source first...
        controller.ack(input).unwrap(); // ...then the controller
        let state = buttons.read(Channel::One).unwrap() & 0xF;
        let pressed = state & !last; // buttons that went down
        last = state;
        if pressed & 0b0111 == 0 {
            continue;
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
        leds.write(Channel::One, count).unwrap();
        println!("buttons {pressed:04b} pressed: count {count}");
    }
    controller.disable(input).unwrap();
    leds.write(Channel::One, 0).unwrap();
    println!("done");
    halt()
}
