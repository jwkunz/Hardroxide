//! Every peripheral of the loopback design against its partner inside the PL, through the generated crate's drivers,
//! with the interrupts taken through the processor's interrupt controller.
use std::time::Duration;
use hardroxide_irq::UioIrq;
use design_1::{Design, pac};
use embedded_hal::i2c::I2c;
use embedded_hal::spi::{MODE_0, SpiBus};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::Mmio;
use hardroxide_mmio::time::Instant;

const SOON: Option<Duration> = Some(Duration::from_millis(200));
// AXI IIC registers the driver (a controller only) does not use: this test makes the second core a target.
const IIC_ADDRESS: usize = 0x110;
const IIC_STATUS: usize = 0x104;
const IIC_TRANSMIT: usize = 0x108;
const IIC_RECEIVE: usize = 0x10C;
/// The stream FIFO's "receive complete" interrupt.
const FIFO_RECEIVED: u32 = 1 << 26;

fn line(l: &hardroxide_irq::IrqLine) -> UioIrq {
    l.open().expect("the interrupt's UIO device")
}

fn arrived(irq: &UioIrq, what: &str) {
    assert!(
        irq.wait_event(SOON).unwrap().is_some(),
        "no {what} interrupt"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut d = Design::load()?;
    let gpio_irq = line(&pac::axi_gpio_0::IRQ_IP2INTC_IRPT);
    let timer_irq = line(&pac::axi_timer_0::IRQ_INTERRUPT);
    let uart_irq = line(&pac::axi_uartlite_1::IRQ_INTERRUPT);
    let fifo_irq = line(&pac::axi_fifo_mm_s_0::IRQ_INTERRUPT);

    // GPIO 1's outputs are GPIO 0's inputs.
    let (input, output) = (&d.axi_gpio_0, &d.axi_gpio_1);
    output.set_input_mask(Channel::One, 0).unwrap();
    output.write(Channel::One, 0).unwrap();
    input.clear_interrupt(Channel::One).unwrap();
    input.enable_interrupt(Channel::One).unwrap();
    gpio_irq.enable().unwrap();
    output.write(Channel::One, 0xA5).unwrap();
    arrived(&gpio_irq, "GPIO");
    assert_eq!(input.read(Channel::One).unwrap(), 0xA5);
    input.clear_interrupt(Channel::One).unwrap();
    println!("GPIO loop and interrupt: ok");

    let timer = &d.axi_timer_0;
    timer_irq.enable().unwrap();
    let t0 = Instant::now();
    timer
        .start(timer.ticks_for_ns(20_000_000) as u32, false, true)
        .unwrap();
    arrived(&timer_irq, "timer");
    let took = t0.elapsed().as_micros();
    timer.clear().unwrap();
    assert!((19_990..20_100).contains(&took), "timer took {took} us");
    println!("timer interrupt after {took} us: ok");

    let mut block = [0u32; 256];
    let ram = &d.axi_bram_ctrl_0;
    let words: [u32; 256] = std::array::from_fn(|i| (i as u32).wrapping_mul(0x9E37_79B9));
    ram.write_words(ram.len() - 1024, &words).unwrap();
    ram.read_words(ram.len() - 1024, &mut block).unwrap();
    assert_eq!(block, words);
    println!("block RAM: ok");

    // The two serial ports are wired to each other.
    let (a, b) = (&d.axi_uartlite_0, &d.axi_uartlite_1);
    a.init().unwrap();
    b.init().unwrap();
    for (name, tx, rx) in [("0 to 1", a, b), ("1 to 0", b, a)] {
        let data: [u8; 256] = std::array::from_fn(|i| i as u8);
        let mut got = [0u8; 256];
        for (chunk, back) in data.chunks(8).zip(got.chunks_mut(8)) {
            tx.send(chunk, Duration::from_secs(1)).unwrap();
            let mut n = 0;
            while n < back.len() {
                n += rx.receive(&mut back[n..], Duration::from_secs(1)).unwrap();
            }
        }
        assert_eq!(got, data, "UART {name}");
        println!("UART {name}: 256 bytes: ok");
    }
    b.set_interrupts(true).unwrap();
    uart_irq.enable().unwrap();
    a.send(&[0x42], Duration::from_secs(1)).unwrap();
    arrived(&uart_irq, "UART receive");
    assert_eq!(b.try_read_byte().unwrap(), Some(0x42));
    println!("UART receive interrupt: ok");

    // The second I2C core listens at 0x2A; the first writes to it.
    d.axi_iic_1.init().unwrap();
    d.axi_iic_1.regs().write32(IIC_ADDRESS, 0x2A << 1).unwrap();
    d.axi_iic_0.init().unwrap();
    let payload = [0xA5u8, 0x5A, 0x3C, 0xFF, 0x00];
    let controller = d.axi_iic_0.regs();
    controller.write32(IIC_TRANSMIT, 0x100 | 0x2A << 1).unwrap();
    for (i, byte) in payload.iter().enumerate() {
        let stop = if i + 1 == payload.len() { 0x200 } else { 0 };
        controller
            .write32(IIC_TRANSMIT, stop | u32::from(*byte))
            .unwrap();
    }
    let mut got = [0u8; 5];
    let (mut n, t0) = (0, Instant::now());
    while n < got.len() && t0.elapsed() < Duration::from_secs(1) {
        // Bit 6: the receive FIFO is empty. The target holds the bus until it is read.
        if d.axi_iic_1.regs().read32(IIC_STATUS).unwrap() & 1 << 6 == 0 {
            got[n] = d.axi_iic_1.regs().read32(IIC_RECEIVE).unwrap() as u8;
            n += 1;
        }
    }
    assert_eq!(got, payload, "I2C target");
    // Bit 2: the bus is busy.
    while d.axi_iic_0.regs().read32(IIC_STATUS).unwrap() & 1 << 2 != 0
        && t0.elapsed() < Duration::from_secs(1)
    {}
    assert!(d.axi_iic_0.write(0x55, &[1]).is_err(), "an absent address");
    println!("I2C write to a target, and an absent address refused: ok");

    // SPI: the output is wired back to the input.
    let spi = &mut d.axi_quad_spi_0;
    spi.init(MODE_0, false, false).unwrap();
    spi.set_chip_select(Some(0)).unwrap();
    let sent: [u8; 40] = std::array::from_fn(|i| (i as u8).wrapping_mul(13).wrapping_add(5));
    let mut buf = sent;
    spi.transfer_in_place(&mut buf).unwrap();
    assert_eq!(buf, sent, "SPI loop");
    println!("SPI, 40 bytes: ok");

    // The stream FIFO's output is its input.
    let fifo = &d.axi_fifo_mm_s_0;
    fifo.reset().unwrap();
    fifo.clear_interrupts(u32::MAX).unwrap();
    fifo.set_interrupt_mask(FIFO_RECEIVED).unwrap();
    fifo_irq.enable().unwrap();
    let packet: [u8; 64] = std::array::from_fn(|i| (i as u8) ^ 0x5A);
    fifo.send(&packet).unwrap();
    arrived(&fifo_irq, "FIFO");
    let mut back = [0u8; 64];
    let length = fifo.recv_into(&mut back, Duration::from_secs(1)).unwrap();
    assert_eq!((length, back), (Some(64), packet), "FIFO packet");
    println!("stream FIFO packet and interrupt: ok");

    println!("LOOPBACK OK");
    Ok(())
}
