//! Base overlay, bare metal: the LEDs, the buttons and the switches, with no operating system.
//!
//! The four LEDs walk once, then for 20 seconds they follow the four buttons, and the switches choose the colour of the
//! two colour LEDs: 0 off, 1 red, 2 green, 3 blue. Each change is printed. Bare metal tutorial 1.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::time::{Instant, sleep};
use ps7::println;

ps7::entry!(main);

/// One colour LED's bits are blue (1), green (2) and red (4); the second LED's follow the first's.
const COLOURS: [u32; 4] = [0b000, 0b100, 0b010, 0b001];

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let (buttons, switches, leds, colour) = (
        &d.btns_gpio,
        &d.switches_gpio,
        &d.leds_gpio,
        &d.rgbleds_gpio,
    );
    // A GPIO pin is an input until told otherwise.
    leds.set_input_mask(Channel::One, 0).unwrap();
    colour.set_input_mask(Channel::One, 0).unwrap();

    println!("\n== leds: the LEDs, one at a time ==");
    for n in 0..4 {
        leds.write(Channel::One, 1 << n).unwrap();
        sleep(Duration::from_millis(250));
    }

    println!("== press the buttons and move the switches (20 s) ==");
    let start = Instant::now();
    let mut last = None;
    while start.elapsed() < Duration::from_secs(20) {
        let pressed = buttons.read(Channel::One).unwrap() & 0xF;
        let setting = switches.read(Channel::One).unwrap() & 0x3;
        leds.write(Channel::One, pressed).unwrap();
        let bits = COLOURS[setting as usize];
        colour.write(Channel::One, bits | bits << 3).unwrap();
        if last != Some((pressed, setting)) {
            println!("buttons {pressed:04b}, switches {setting:02b}");
            last = Some((pressed, setting));
        }
        sleep(Duration::from_millis(20));
    }
    leds.write(Channel::One, 0).unwrap();
    colour.write(Channel::One, 0).unwrap();
    println!("done");
    halt()
}
