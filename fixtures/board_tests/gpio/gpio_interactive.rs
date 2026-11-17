//! Walk the four LEDs one at a time (2 s each) and print every change of the
//! buttons and switches on channel 2. Press buttons and flip switches while it
//! runs (45 s). Run as root on the board.
use hardroxide_ip::gpio::Channel;
use std::{
    io::Write,
    thread::sleep,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let g = &d.axi_gpio_0;
    g.set_input_mask(Channel::One, 0)?;
    let t0 = Instant::now();
    let mut last = u32::MAX;
    let mut step = 0u32;
    let mut next = Instant::now();
    while t0.elapsed() < Duration::from_secs(45) {
        if Instant::now() >= next {
            g.write(Channel::One, 1 << (step % 4))?;
            println!(
                "[{:>4.1}s] LED{} ON (pattern {:#06b})",
                t0.elapsed().as_secs_f32(),
                step % 4,
                1 << (step % 4)
            );
            step += 1;
            next += Duration::from_millis(2000);
        }
        let v = g.read(Channel::Two)?;
        if v != last {
            println!(
                "[{:>4.1}s] inputs {:#04x}  btn[3:0]={:04b} sw[1:0]={:02b}",
                t0.elapsed().as_secs_f32(),
                v,
                v & 0xF,
                (v >> 4) & 3
            );
            last = v;
        }
        std::io::stdout().flush()?;
        sleep(Duration::from_millis(5));
    }
    g.write(Channel::One, 0)?;
    Ok(())
}
