//! UART interrupts: wait for a received byte instead of polling, and for the
//! transmit-empty interrupt. Needs `interrupts.dtso` applied.
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let (a, b) = (&d.axi_uartlite_0, &d.axi_uartlite_1);
    a.init()?;
    b.init()?;
    let irq_b = design_1::pac::axi_uartlite_1::IRQ_INTERRUPT.open()?;
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
            if irq_b.wait(Some(Duration::from_millis(200)))?.is_none() {
                break;
            }
            if let Some(x) = b.try_read_byte()? {
                got = Some(x);
                break;
            }
        }
        println!(
            "round {round}: irq-driven RX byte {got:x?} after {:?}",
            t.elapsed()
        );
        assert_eq!(got, Some(0x40 + round));
    }
    // idle: no event expected once RX is drained and TX is empty? report
    let idle = irq_b.wait(Some(Duration::from_millis(300)))?;
    println!("idle wait -> {idle:?}");
    // TX-empty interrupt on b (its own line; line SPI 29 of uart 0 is taken by the base design's mapping)
    b.set_interrupts(true)?;
    while b.try_read_byte()?.is_some() {}
    let _ = irq_b.wait(Some(Duration::from_millis(50)))?;
    b.try_write_byte(0x55)?;
    let t = Instant::now();
    let ev = irq_b.wait(Some(Duration::from_secs(1)))?;
    println!(
        "TX-empty interrupt on b: {ev:?} after {:?} (a byte at 115200 baud takes ~87 us)",
        t.elapsed()
    );
    // frame/overrun flags stay clear
    println!("errors a {:?} b {:?}", a.errors()?, b.errors()?);
    println!("total {:?}", t0.elapsed());
    Ok(())
}
