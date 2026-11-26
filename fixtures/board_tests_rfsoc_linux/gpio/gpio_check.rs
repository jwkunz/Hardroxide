//! The LEDs (written and read back), the buttons and switches, the colour LEDs, and the inputs' interrupt, which a
//! write to the GPIO's status register raises without anyone pressing anything.
use design_1::{Design, pac};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::Mmio;
use std::time::Duration;

/// The AXI GPIO's interrupt status register: writing 1 to a bit toggles it.
const STATUS: usize = 0x120;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = Design::load()?;
    let (io, rgb) = (&d.axi_gpio_0, &d.axi_gpio_1);
    io.set_input_mask(Channel::One, 0)?;
    rgb.set_input_mask(Channel::One, 0)?;
    for pattern in [0x5, 0xA, 0xF, 0] {
        io.write(Channel::One, pattern)?;
        assert_eq!(io.read(Channel::One)?, pattern, "LEDs");
    }
    for pattern in [0x15, 0x2A, 0x3F, 0] {
        rgb.write(Channel::One, pattern)?;
        assert_eq!(rgb.read(Channel::One)?, pattern, "colour LEDs");
    }
    let inputs = io.read(Channel::Two)?;
    println!("buttons {:05b}, switches {:04b}", inputs & 0x1F, inputs >> 5);

    let irq = pac::axi_gpio_0::IRQ_IP2INTC_IRPT.open()?;
    for round in 0..3 {
        io.clear_interrupt(Channel::Two)?;
        io.enable_interrupt(Channel::Two)?;
        // Nothing pending before the status bit is set...
        irq.enable()?;
        assert!(irq.wait_event(Some(Duration::from_millis(50)))?.is_none(), "early in round {round}");
        io.regs().write32(STATUS, 2)?;
        assert!(irq.wait_event(Some(Duration::from_millis(100)))?.is_some(), "no interrupt in round {round}");
        io.clear_interrupt(Channel::Two)?;
    }
    println!("input interrupt: 3 of 3");
    println!("GPIO OK");
    Ok(())
}
