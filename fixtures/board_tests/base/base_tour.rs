//! A crate generated from the PYNQ base overlay (a design you did not build):
//! read the switches and buttons, walk the LEDs, look at the interrupt controller.
//! Run as root on the board: `sudo ./base_tour`.
use hardroxide_ip::gpio::Channel;
use std::{thread::sleep, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = base::Design::load()?;
    println!("loaded the PYNQ base overlay through a generated crate");
    println!(
        "switches (2 bits): {:#x}",
        d.switches_gpio.read(Channel::One)?
    );
    println!("buttons  (4 bits): {:#x}", d.btns_gpio.read(Channel::One)?);

    d.leds_gpio.set_input_mask(Channel::One, 0)?; // channel 1: all outputs
    for step in 0..8 {
        d.leds_gpio.write(Channel::One, 1 << (step % 4))?;
        sleep(Duration::from_millis(250));
    }
    d.leds_gpio.write(Channel::One, 0)?;

    println!(
        "system_interrupts: enabled {:#x}, pending {:#x}",
        d.system_interrupts.enabled()?,
        d.system_interrupts.pending()?
    );
    Ok(())
}
