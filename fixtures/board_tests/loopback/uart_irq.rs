use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let (a, b) = (&d.axi_uartlite_0, &d.axi_uartlite_1);
    a.init()?; b.init()?;
    let irq_b = design_1::pac::axi_uartlite_1::IRQ_INTERRUPT.open()?;
    let irq_a = design_1::pac::axi_uartlite_0::IRQ_INTERRUPT.open()?;
    // RX interrupt on b: a sends a byte.
    b.set_interrupts(true)?;
    let t0 = Instant::now();
    // Interrupt enable with an empty TX FIFO may fire at once (TX-empty); drain that first.
    let first = irq_b.wait(Some(Duration::from_millis(200)))?;
    println!("after enable (TX empty, nothing sent): event {first:?}");
    let _ = b.try_read_byte()?;
    for round in 0..3u8 {
        // clear any pending, then send
        while b.try_read_byte()?.is_some() {}
        let _ = irq_b.wait(Some(Duration::from_millis(5)))?; // flush a stale TX-empty event
        a.try_write_byte(0x40 + round)?;
        let t = Instant::now();
        let mut got = None;
        while t.elapsed() < Duration::from_secs(1) {
            if irq_b.wait(Some(Duration::from_millis(200)))?.is_none() { break; }
            if let Some(x) = b.try_read_byte()? { got = Some(x); break; }
        }
        println!("round {round}: irq-driven RX byte {got:x?} after {:?}", t.elapsed());
        assert_eq!(got, Some(0x40 + round));
    }
    // idle: no event expected once RX is drained and TX is empty? report
    let idle = irq_b.wait(Some(Duration::from_millis(300)))?;
    println!("idle wait -> {idle:?}");
    // TX-empty interrupt on a
    a.set_interrupts(true)?;
    let _ = irq_a.wait(Some(Duration::from_millis(50)))?;
    a.try_write_byte(0x55)?;
    let t = Instant::now();
    let ev = irq_a.wait(Some(Duration::from_secs(1)))?;
    println!("TX-empty interrupt on a: {ev:?} after {:?}", t.elapsed());
    println!("total {:?}", t0.elapsed());
    Ok(())
}
