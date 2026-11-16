use std::time::Duration;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let v = &d.vadd_0;
    let irq = design_1::pac::vadd_0::IRQ_INTERRUPT.open()?;
    v.enable_done_interrupt()?;
    for i in 0..3u32 {
        v.set_a(i)?; v.set_b(10)?;
        v.start()?;
        let r = irq.wait(Some(Duration::from_secs(2)))?;
        let flags = v.ack_interrupts()?;
        println!("round {i}: event {r:?} flags {flags:#x} out={}", v.out_r()?);
    }
    Ok(())
}
