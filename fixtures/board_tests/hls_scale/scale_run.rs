use hardroxide_dma::{DmaBuffer, Udmabuf};
use std::time::Duration;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let s = &d.scale_0;
    let mut a = Udmabuf::open(0)?;
    let mut b = Udmabuf::open(1)?;
    let n = 256usize;
    for (i, c) in a.as_mut_slice()[..n * 4].chunks_mut(4).enumerate() { c.copy_from_slice(&(i as i32 - 100).to_le_bytes()); }
    b.as_mut_slice()[..n * 4].fill(0xEE);
    s.set_in_r_buffer(&a)?; s.set_out_r_buffer(&b)?;
    s.set_n(n as u32)?; s.set_k(7)?;
    s.start()?;
    s.wait_done(Duration::from_secs(1))?;
    let mut bad = 0;
    for (i, c) in b.as_slice()[..n * 4].chunks(4).enumerate() {
        let got = i32::from_le_bytes(c.try_into().unwrap());
        if got != (i as i32 - 100) * 7 { bad += 1; if bad < 4 { println!("mismatch at {i}: {got}"); } }
    }
    println!("scale: {n} elements, {bad} mismatches");
    Ok(())
}
