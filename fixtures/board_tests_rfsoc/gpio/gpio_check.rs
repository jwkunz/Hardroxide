//! The LEDs (written and read back), the buttons and switches, the colour LEDs, and the inputs' interrupt, which a
//! write to the GPIO's status register raises without anyone pressing anything.
#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, psu};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::Mmio;
use psu::println;
use psu::wait::IrqWait;

psu::entry!(main);

/// The AXI GPIO's interrupt status register: writing 1 to a bit toggles it.
const STATUS: usize = 0x120;

fn main() -> ! {
    let d = Design::init().expect("design");
    let (io, rgb) = (&d.axi_gpio_0, &d.axi_gpio_1);
    io.set_input_mask(Channel::One, 0).unwrap();
    rgb.set_input_mask(Channel::One, 0).unwrap();
    for pattern in [0x5, 0xA, 0xF, 0] {
        io.write(Channel::One, pattern).unwrap();
        assert_eq!(io.read(Channel::One).unwrap(), pattern, "LEDs");
    }
    for pattern in [0x15, 0x2A, 0x3F, 0] {
        rgb.write(Channel::One, pattern).unwrap();
        assert_eq!(rgb.read(Channel::One).unwrap(), pattern, "colour LEDs");
    }
    let inputs = io.read(Channel::Two).unwrap();
    println!(
        "buttons {:05b}, switches {:04b}",
        inputs & 0x1F,
        inputs >> 5
    );

    let irq = IrqWait::for_line(&design_1::pac::axi_gpio_0::IRQ_IP2INTC_IRPT).expect("interrupt");
    d.enable_interrupts();
    for round in 0..3 {
        io.clear_interrupt(Channel::Two).unwrap();
        io.enable_interrupt(Channel::Two).unwrap();
        irq.enable().unwrap();
        io.regs().write32(STATUS, 2).unwrap();
        let event = irq.wait_event(Some(Duration::from_millis(100))).unwrap();
        assert!(event.is_some(), "no interrupt in round {round}");
        io.clear_interrupt(Channel::Two).unwrap();
    }
    println!("input interrupt: 3 of 3");
    println!("GPIO OK");
    design_1::halt()
}
