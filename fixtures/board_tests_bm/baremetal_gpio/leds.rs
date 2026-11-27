//! Bare metal, no operating system: mirror the buttons and switches onto the LEDs.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::time::sleep;
use ps7::println;

ps7::entry!(main);

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let gpio = &d.axi_gpio_0;
    gpio.set_input_mask(Channel::One, 0).unwrap(); // channel 1: LEDs, all outputs
    println!("\n== leds: buttons and switches -> LEDs for 20 s (press something) ==");
    let mut last = u32::MAX;
    for _ in 0..400 {
        let inputs = gpio.read(Channel::Two).unwrap(); // buttons 3:0, switches 5:4
        gpio.write(Channel::One, inputs & 0xF).unwrap();
        if inputs != last {
            println!("buttons {:04b}, switches {:02b}", inputs & 0xF, inputs >> 4);
            last = inputs;
        }
        sleep(Duration::from_millis(50));
    }
    println!("done");
    halt()
}
