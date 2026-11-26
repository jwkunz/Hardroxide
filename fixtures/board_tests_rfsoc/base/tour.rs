//! The base overlay through its generated crate: LEDs, the buttons' and switches' interrupts through the AXI interrupt
//! controller (raised from software), the Pmod processors' program memories, and the receive DMAs being there.
//! The radio blocks in the converters' clock domains are not touched.
#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, pac, psu};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::Mmio;
use psu::println;
use psu::wait::IrqWait;

psu::entry!(main);

/// An AXI GPIO's interrupt status register: writing 1 to a bit toggles it.
const STATUS: usize = 0x120;
/// The system monitor's temperature reading.
const TEMPERATURE: usize = 0x400;

fn main() -> ! {
    let d = Design::init().expect("design");
    let reading = d.system_management_wiz_0.read32(TEMPERATURE).unwrap() as u16;
    println!(
        "FPGA temperature: {} C",
        psu::sysmon::millicelsius(reading) / 1000
    );
    for (leds, mask) in [(&d.leds_gpio, 0xF), (&d.rgbleds_gpio, 0x3F)] {
        leds.set_input_mask(Channel::One, 0).unwrap();
        for pattern in [0x15 & mask, 0x2A & mask, mask, 0] {
            leds.write(Channel::One, pattern).unwrap();
            assert_eq!(leds.read(Channel::One).unwrap(), pattern, "LEDs");
        }
    }
    println!("LEDs: ok");

    let irq = IrqWait::for_line(&pac::axi_intc_0::IRQ_IRQ).expect("the controller's output");
    d.enable_interrupts();
    let controller = &d.axi_intc_0;
    controller.init().unwrap();
    for (name, gpio, line) in [
        ("buttons", &d.btns_gpio, &pac::btns_gpio::IRQ_IP2INTC_IRPT),
        ("switches", &d.sws_gpio, &pac::sws_gpio::IRQ_IP2INTC_IRPT),
    ] {
        let (_, bit) = line.via_intc.expect("behind the controller");
        gpio.clear_interrupt(Channel::One).unwrap();
        gpio.enable_interrupt(Channel::One).unwrap();
        controller.ack(1 << bit).unwrap();
        controller.enable(1 << bit).unwrap();
        gpio.regs().write32(STATUS, 1).unwrap();
        let pending = controller
            .wait(&irq, 1 << bit, Some(Duration::from_millis(100)))
            .unwrap();
        assert_eq!(pending, Some(1 << bit), "{name}");
        gpio.clear_interrupt(Channel::One).unwrap();
        controller.ack(1 << bit).unwrap();
        controller.disable(1 << bit).unwrap();
        println!("{name} interrupt through the controller (input {bit}): ok");
    }

    for (n, ram) in [&d.iop_pmod0_mb_bram_ctrl, &d.iop_pmod1_mb_bram_ctrl]
        .into_iter()
        .enumerate()
    {
        let words: [u32; 128] =
            core::array::from_fn(|i| (i as u32 + n as u32).wrapping_mul(0x9E37_79B9));
        let mut back = [0u32; 128];
        ram.write_words(0xF000, &words).unwrap();
        ram.read_words(0xF000, &mut back).unwrap();
        assert_eq!(back, words, "Pmod {n} memory");
    }
    println!("Pmod processor memories: ok");
    println!("BASE OK");
    design_1::halt()
}
