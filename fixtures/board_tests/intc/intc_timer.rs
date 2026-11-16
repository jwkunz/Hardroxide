use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let (t, c) = (&d.axi_timer_0, &d.axi_intc_0);
    let irq = design_1::pac::axi_intc_0::IRQ_IRQ.open()?;
    c.init()?;
    c.enable(1 << 0)?;
    println!("enabled {:#x}", c.enabled()?);
    for round in 0..3 {
        t.clear()?;
        let t0 = Instant::now();
        t.start(t.ticks_for_ns(30_000_000) as u32, false, true)?;
        let p = c.wait(&irq, 1, Some(Duration::from_secs(2)))?;
        println!("round {round}: pending {p:?} after {:?}", t0.elapsed());
        t.clear()?;
        c.ack(1)?;
        println!("  after ack pending={:#x}", c.pending()?);
    }
    // timeout path
    let p = c.wait(&irq, 1, Some(Duration::from_millis(100)))?;
    println!("idle wait -> {p:?}");
    Ok(())
}
