use hardroxide_mmio::Mmio;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // G3: leave the timer running, drop the Design, load again: the PL is
    // reprogrammed, so the timer must be back at its reset state.
    {
        let d = design_1::Design::load()?;
        let t = &d.axi_timer_0;
        t.start(t.ticks_for_ns(5_000_000_000) as u32, true, false)?;
        println!("before reload: TCSR0 {:#x}, counter {}", t.regs().read32(0)?, t.counter()?);
        // G7: a second load while this one is alive must be refused
        match design_1::Design::load() {
            Err(design_1::Error::AlreadyLoaded) => println!("second load while alive: AlreadyLoaded (ok)"),
            other => println!("second load while alive: UNEXPECTED {:?}", other.map(|_| ())),
        }
        println!("still working after the refused load: counter {}", t.counter()?);
    }
    let d = design_1::Design::load()?;
    let t = &d.axi_timer_0;
    println!("after reload: TCSR0 {:#x}, TLR0 {:#x}, counter {}", t.regs().read32(0)?, t.regs().read32(4)?, t.counter()?);
    drop(d);
    // G7: many load/drop cycles in one process
    let t0 = Instant::now();
    let mut worst = 0.0f64;
    for i in 0..25 {
        let s = Instant::now();
        let d = design_1::Design::load()?;
        let load = s.elapsed().as_secs_f64();
        worst = worst.max(load);
        let t = &d.axi_timer_0;
        let m = Instant::now();
        t.delay_ticks(t.ticks_for_ns(10_000_000))?;
        let took = m.elapsed().as_secs_f64() * 1e3;
        if !(9.9..10.3).contains(&took) { println!("cycle {i}: delay {took:.3} ms OUT OF RANGE"); }
    }
    println!("25 load/delay/drop cycles in {:?}, slowest load {:.0} ms", t0.elapsed(), worst * 1e3);
    Ok(())
}
