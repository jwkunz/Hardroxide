//! IIC interrupt: the NACK status flag raises the core's interrupt. Needs
//! `interrupts.dtso` applied.
use hardroxide_mmio::Mmio;
use std::time::Duration;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let m = &d.axi_iic_0;
    m.init()?;
    let irq = design_1::pac::axi_iic_0::IRQ_IIC2INTC_IRPT.open()?;
    let r = m.regs();
    println!("ISR after init {:#x}", r.read32(0x20)?);
    // NACK interrupt (int1): write to an address nobody answers.
    r.write32(0x28, 1 << 1)?; // IER: transmit error
    r.write32(0x1C, 1 << 31)?; // GIE
    for round in 0..3 {
        r.write32(0x108, 0x100 | (0x55 << 1))?;
        r.write32(0x108, 0x200 | 0x01)?;
        let ev = irq.wait(Some(Duration::from_secs(1)))?;
        let isr = r.read32(0x20)?;
        println!(
            "round {round}: event {ev:?}, ISR {isr:#x}, NACK flag {}",
            isr & 2 != 0
        );
        r.write32(0x20, isr & 2)?; // toggle-clear just that bit
        m.init()?;
        r.write32(0x28, 1 << 1)?;
        r.write32(0x1C, 1 << 31)?;
    }
    println!(
        "idle wait -> {:?}",
        irq.wait(Some(Duration::from_millis(300)))?
    );
    Ok(())
}
