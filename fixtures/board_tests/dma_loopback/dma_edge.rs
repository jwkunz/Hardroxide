use hardroxide_dma::{DmaBuffer, Udmabuf};
use hardroxide_ip::dma::Direction;
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let dma = &d.axi_dma_0;
    dma.reset()?;
    println!("max transfer length: {} bytes", dma.max_transfer_len());
    let mut tx = Udmabuf::open(0)?;
    let mut rx = Udmabuf::open(1)?;
    println!("buffers: tx {} bytes @ {:#x}, rx {} bytes @ {:#x}", tx.len(), tx.phys_addr(), rx.len(), rx.phys_addr());
    // 1. Timeout: receive with nothing sent. The channel must be halted and reusable.
    rx.as_mut_slice()[..64].fill(0);
    let t = dma.start(Direction::StreamToMem, rx, 64).map_err(|e| e.error)?;
    let t0 = Instant::now();
    let r = t.wait(Duration::from_millis(100));
    println!("S2MM with no sender: {:?} after {:?}", r.as_ref().err(), t0.elapsed());
    assert!(r.is_err());
    let mut rx = Udmabuf::open(1)?; // the buffer was dropped by abort(); reopen
    // 2. Recovery: a normal loopback right after.
    for (i, b) in tx.as_mut_slice()[..256].iter_mut().enumerate() { *b = i as u8; }
    let rt = dma.start(Direction::StreamToMem, rx, 256).map_err(|e| e.error)?;
    let tt = dma.start(Direction::MemToStream, tx, 256).map_err(|e| e.error)?;
    tx = tt.wait(Duration::from_secs(1))?;
    rx = rt.wait(Duration::from_secs(1))?;
    println!("recovery loopback ok: {}", tx.as_slice()[..256] == rx.as_slice()[..256]);
    // 3. Lengths, including unaligned ones.
    for n in [4usize, 8, 60, 1000, 4096, 65536, 1 << 20] {
        let n = n.min(tx.len()).min(rx.len());
        for (i, b) in tx.as_mut_slice()[..n].iter_mut().enumerate() { *b = (i as u32).wrapping_mul(2654435761) as u8; }
        rx.as_mut_slice()[..n].fill(0xEE);
        let rt = dma.start(Direction::StreamToMem, rx, n).map_err(|e| e.error)?;
        let tt = dma.start(Direction::MemToStream, tx, n).map_err(|e| e.error)?;
        let t0 = Instant::now();
        tx = tt.wait(Duration::from_secs(2))?;
        rx = rt.wait(Duration::from_secs(2))?;
        let dt = t0.elapsed();
        println!("{n:>8} bytes: ok={} in {:?} ({:.1} MB/s)", tx.as_slice()[..n] == rx.as_slice()[..n], dt, n as f64 / dt.as_secs_f64() / 1e6);
    }
    // 4. Many back-to-back transfers.
    let n = 4096;
    let t0 = Instant::now();
    let mut bad = 0;
    for round in 0..200u32 {
        for (i, b) in tx.as_mut_slice()[..n].iter_mut().enumerate() { *b = (i as u32 + round) as u8; }
        let rt = dma.start(Direction::StreamToMem, rx, n).map_err(|e| e.error)?;
        let tt = dma.start(Direction::MemToStream, tx, n).map_err(|e| e.error)?;
        tx = tt.wait(Duration::from_secs(1))?;
        rx = rt.wait(Duration::from_secs(1))?;
        if tx.as_slice()[..n] != rx.as_slice()[..n] { bad += 1; }
    }
    println!("200 x 4096-byte loopbacks: {bad} mismatches, {:?}", t0.elapsed());
    Ok(())
}
