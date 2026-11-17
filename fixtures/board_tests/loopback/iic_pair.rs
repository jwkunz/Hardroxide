//! Two AXI IIC cores on one in-fabric bus: core 1 is a target at address
//! `0x2A`, core 0 writes five bytes to it, an unanswered address NACKs, and a bus
//! scan finds only `0x2A`.
use embedded_hal::i2c::I2c;
use hardroxide_mmio::Mmio;
use std::time::{Duration, Instant};
const ADR: usize = 0x110;
const SR: usize = 0x104;
const TXF: usize = 0x108;
const RXF: usize = 0x10C;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut d = design_1::Design::load()?;
    d.axi_iic_1.init()?;
    d.axi_iic_1.regs().write32(ADR, 0x2A << 1)?;
    d.axi_iic_0.init()?;
    // 1. Master writes 5 bytes to the target at 0x2A. The target's core holds
    // the bus until its RX FIFO is read, so queue the bytes and drain it.
    let payload = [0xA5u8, 0x5A, 0x3C, 0xFF, 0x00];
    let m = d.axi_iic_0.regs();
    m.write32(TXF, 0x100 | (0x2A << 1))?; // START + address + W
    for (i, b) in payload.iter().enumerate() {
        m.write32(
            TXF,
            if i + 1 == payload.len() {
                0x200 | *b as u32
            } else {
                *b as u32
            },
        )?;
    }
    let mut got = Vec::new();
    let t0 = Instant::now();
    while got.len() < payload.len() && t0.elapsed() < Duration::from_secs(1) {
        if d.axi_iic_1.regs().read32(SR)? & (1 << 6) == 0 {
            got.push((d.axi_iic_1.regs().read32(RXF)? & 0xFF) as u8);
        }
    }
    while m.read32(SR)? & (1 << 2) != 0 && t0.elapsed() < Duration::from_secs(1) {}
    println!(
        "target received {got:02x?} (sent {payload:02x?}) match={}",
        got == payload
    );
    // 2. An address nobody answers must NACK.
    println!("write to 0x55 -> {:?}", d.axi_iic_0.write(0x55, &[1]));
    // 3. Scan: only the target should answer.
    let mut found = vec![];
    for a in 0x08..0x78u8 {
        if d.axi_iic_0.write(a, &[]).is_ok() {
            found.push(a);
        }
    }
    println!("scan found {found:02x?}");
    Ok(())
}
