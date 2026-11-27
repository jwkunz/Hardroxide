//! The base overlay under Linux: temperature, LEDs, the buttons' and switches' interrupts through the AXI interrupt
//! controller (raised from software) and the Pmod processors' program memories. The radio blocks are not touched.
use design_1::{Design, pac};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::Mmio;
use std::time::Duration;

const STATUS: usize = 0x120;
const TEMPERATURE: usize = 0x400;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = Design::load()?;
    let reading = d.system_management_wiz_0.read32(TEMPERATURE)? as u16;
    // The system monitor's transfer function for this family.
    println!("FPGA temperature: {:.1} C", f64::from(reading) * 509.3140064 / 65536.0 - 280.23087870);
    for (leds, mask) in [(&d.leds_gpio, 0xF), (&d.rgbleds_gpio, 0x3F)] {
        leds.set_input_mask(Channel::One, 0)?;
        for pattern in [0x15 & mask, 0x2A & mask, mask, 0] {
            leds.write(Channel::One, pattern)?;
            assert_eq!(leds.read(Channel::One)?, pattern, "LEDs");
        }
    }
    println!("LEDs: ok");

    let irq = pac::axi_intc_0::IRQ_IRQ.open()?;
    let controller = &d.axi_intc_0;
    controller.init()?;
    for round in 0..50 {
        for (name, gpio, line) in [
            ("buttons", &d.btns_gpio, &pac::btns_gpio::IRQ_IP2INTC_IRPT),
            ("switches", &d.sws_gpio, &pac::sws_gpio::IRQ_IP2INTC_IRPT),
        ] {
            let (_, bit) = line.via_intc.expect("behind the controller");
            gpio.clear_interrupt(Channel::One)?;
            gpio.enable_interrupt(Channel::One)?;
            controller.ack(1 << bit)?;
            controller.enable(1 << bit)?;
            gpio.regs().write32(STATUS, 1)?;
            let pending = controller.wait(&irq, 1 << bit, Some(Duration::from_millis(100)))?;
            assert_eq!(pending, Some(1 << bit), "{name}, round {round}");
            gpio.clear_interrupt(Channel::One)?;
            controller.ack(1 << bit)?;
            controller.disable(1 << bit)?;
        }
    }
    println!("buttons and switches interrupts through the controller: 50 of 50 each");

    for (n, ram) in [&d.iop_pmod0_mb_bram_ctrl, &d.iop_pmod1_mb_bram_ctrl].into_iter().enumerate() {
        let words: [u32; 128] = std::array::from_fn(|i| (i as u32 + n as u32).wrapping_mul(0x9E37_79B9));
        let mut back = [0u32; 128];
        ram.write_words(0xF000, &words)?;
        ram.read_words(0xF000, &mut back)?;
        assert_eq!(back, words, "Pmod {n} memory");
    }
    println!("Pmod processor memories: ok");
    println!("BASE OK");
    Ok(())
}
