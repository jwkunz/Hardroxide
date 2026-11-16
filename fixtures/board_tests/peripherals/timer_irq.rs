use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let t = &d.axi_timer_0;
    let irq = design_1::pac::axi_timer_0::IRQ_INTERRUPT.open()?;
    for round in 0..3 {
        t.clear()?;
        let t0 = Instant::now();
        t.start(t.ticks_for_ns(50_000_000) as u32, false, true)?;
        let r = irq.wait(Some(Duration::from_secs(2)))?;
        println!("round {round}: event {:?} after {:?}, expired={}", r, t0.elapsed(), t.expired()?);
        t.clear()?;
    }
    Ok(())
}
