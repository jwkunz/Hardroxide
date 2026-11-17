//! Drive the Timer through its `/dev/uioN` device instead of `/dev/mem`:
//! the `Uio` backend maps register window 0 of a UIO device.
use hardroxide_ip::timer::AxiTimer;
use hardroxide_mmio::{Mmio, MmioBackend, Uio};
use std::time::Instant;
fn find(name: &str) -> Option<String> {
    for e in std::fs::read_dir("/sys/class/uio").ok()? {
        let p = e.ok()?.path();
        if std::fs::read_to_string(p.join("name")).ok()?.trim() == name {
            return Some(format!("/dev/{}", p.file_name()?.to_str()?));
        }
    }
    None
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Program the PL through the normal path, then talk to the timer ONLY through UIO.
    let d = design_1::Design::load()?;
    let dev = find("axi_timer_0_interrupt").ok_or("no uio device for the timer")?;
    println!("timer uio device: {dev}");
    let region = Uio::new(&dev).map(0, 0x10000)?;
    println!(
        "len {:#x}; TCSR0 via UIO {:#x}, via /dev/mem {:#x}",
        region.len(),
        region.read32(0)?,
        d.axi_timer_0.regs().read32(0)?
    );
    let t = AxiTimer::new(region, 100_000_000);
    let t0 = Instant::now();
    t.delay_ticks(t.ticks_for_ns(100_000_000))?;
    println!(
        "100 ms delay through the UIO backend took {:?}",
        t0.elapsed()
    );
    // both views see the same hardware: start through UIO, observe through /dev/mem
    t.clear()?;
    t.start(100_000_000, false, false)?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    println!(
        "counter via /dev/mem {} (UIO view {})",
        d.axi_timer_0.counter()?,
        t.counter()?
    );
    t.stop()?;
    Ok(())
}
