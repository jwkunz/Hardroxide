//! Base overlay, bare metal: the four buttons act through an interrupt.
//!
//! The buttons' GPIO raises an interrupt when a pin changes; it goes through the AXI interrupt
//! controller (`system_interrupts`) to the processor's GIC. The loop sleeps in `wait` until a
//! press arrives, then clears the GPIO flag, acknowledges the controller and acts:
//! button 0 counts up on the LEDs, button 1 counts down, button 2 clears, button 3 lights the
//! RGB LEDs in the colour the switches choose. The switches are read when a button is pressed.
//! Base overlay application 1.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::time::{Instant, sleep};
use ps7::println;
use ps7::wait::IrqWait;

ps7::entry!(main);

/// Input of `system_interrupts` that the buttons' GPIO feeds (from the design).
const BTNS_BIT: u32 = 1 << 13;

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let (btns, sw, leds, rgb, intc) = (
        &d.btns_gpio,
        &d.switches_gpio,
        &d.leds_gpio,
        &d.rgbleds_gpio,
        &d.system_interrupts,
    );
    leds.set_input_mask(Channel::One, 0).unwrap();
    rgb.set_input_mask(Channel::One, 0).unwrap();
    let irq = IrqWait::for_line(&design_1::pac::system_interrupts::IRQ_IRQ).expect("controller reaches the PS");

    intc.init().unwrap();
    intc.enable(BTNS_BIT).unwrap();
    btns.enable_interrupt(Channel::One).unwrap();
    d.enable_interrupts();

    println!("\n== lights: press the buttons (60 s) ==");
    let (mut count, mut last_state) = (0u32, 0u32);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(60) {
        let Some(_) = intc.wait(&irq, BTNS_BIT, Some(Duration::from_millis(500))).unwrap() else {
            continue;
        };
        sleep(Duration::from_millis(20)); // contacts bounce for a few milliseconds
        let state = btns.read(Channel::One).unwrap() & 0xF;
        btns.clear_interrupt(Channel::One).unwrap(); // clear the source first...
        intc.ack(BTNS_BIT).unwrap(); // ...then the controller
        let pressed = state & !last_state; // buttons that went down
        last_state = state;
        if pressed == 0 {
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
        let colour = sw.read(Channel::One).unwrap() & 0b11;
        if pressed & 0b1000 != 0 {
            // Both RGB LEDs: switches 00 = off, 01 = red, 10 = green, 11 = blue.
            let bits = [0b000, 0b001, 0b010, 0b100][colour as usize];
            rgb.write(Channel::One, bits | bits << 3).unwrap();
        }
        leds.write(Channel::One, count).unwrap();
        println!("buttons {pressed:04b} pressed: LEDs {count:04b}, switches {colour:02b}");
    }
    println!("done");
    halt()
}
