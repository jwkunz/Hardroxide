//! PYNQ-Z2 base overlay under Linux: the LEDs, the buttons and the switches.
//!
//! The four LEDs walk once, then for half a minute (or the number of seconds given as the first argument) they follow
//! the four buttons, and the switches choose the colour of the two colour LEDs: 0 off, 1 red, 2 green, 3 blue. Each
//! change is printed. Run as root. Linux tutorial 1.
use design_1::Design;
use hardroxide_ip::gpio::Channel;
use std::error::Error;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// One colour LED's bits are blue (1), green (2) and red (4); the second LED's follow the first's.
const COLOURS: [u32; 4] = [0b000, 0b100, 0b010, 0b001];

fn main() -> Result<(), Box<dyn Error>> {
    let seconds = match std::env::args().nth(1) {
        Some(s) => s.parse()?,
        None => 30,
    };
    // Loads the bitstream, sets the clocks and maps every block of the design.
    let d = Design::load()?;
    let (buttons, switches, leds, colour) = (
        &d.btns_gpio,
        &d.switches_gpio,
        &d.leds_gpio,
        &d.rgbleds_gpio,
    );
    // A GPIO pin is an input until told otherwise.
    leds.set_input_mask(Channel::One, 0)?;
    colour.set_input_mask(Channel::One, 0)?;

    println!("== the LEDs, one at a time ==");
    for n in 0..4 {
        leds.write(Channel::One, 1 << n)?;
        sleep(Duration::from_millis(250));
    }

    println!("== press the buttons and move the switches ({seconds} s) ==");
    let start = Instant::now();
    let mut last = None;
    while start.elapsed() < Duration::from_secs(seconds) {
        let pressed = buttons.read(Channel::One)? & 0xF;
        let setting = switches.read(Channel::One)? & 0x3;
        leds.write(Channel::One, pressed)?;
        let bits = COLOURS[setting as usize];
        colour.write(Channel::One, bits | bits << 3)?;
        if last != Some((pressed, setting)) {
            println!("buttons {pressed:04b}, switches {setting:02b}");
            last = Some((pressed, setting));
        }
        sleep(Duration::from_millis(20));
    }
    leds.write(Channel::One, 0)?;
    colour.write(Channel::One, 0)?;
    println!("done");
    Ok(())
}
