//! Two AXI UART Lite cores wired TX-to-RX inside the fabric: send all 256
//! byte values from each to the other and compare.
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let (a, b) = (&d.axi_uartlite_0, &d.axi_uartlite_1);
    a.init()?;
    b.init()?;
    let t = Duration::from_secs(1);
    for (name, tx, rx) in [("0 -> 1", a, b), ("1 -> 0", b, a)] {
        let data: Vec<u8> = (0..=255u8).collect();
        let mut got = Vec::new();
        let t0 = Instant::now();
        for chunk in data.chunks(8) {
            tx.send(chunk, t)?;
            let mut buf = vec![0u8; chunk.len()];
            let mut n = 0;
            while n < buf.len() {
                n += rx.receive(&mut buf[n..], t)?;
            }
            got.extend_from_slice(&buf);
        }
        println!(
            "uart {name}: {} bytes in {:?}, match = {}, errors = {:?}",
            got.len(),
            t0.elapsed(),
            got == data,
            rx.errors()?
        );
        assert_eq!(got, data);
    }
    Ok(())
}
