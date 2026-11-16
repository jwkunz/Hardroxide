use hardroxide_ip::fifo::FifoError;
use std::time::Duration;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let f = &d.axi_fifo_mm_s_0;
    f.reset()?;
    let t = Duration::from_millis(500);
    println!("tx vacancy {} words, rx occupancy {}", f.tx_vacancy()?, f.rx_occupancy()?);
    // odd and small lengths
    for len in [1usize, 2, 3, 4, 5, 7, 8, 9, 63, 64, 65] {
        let p: Vec<u8> = (0..len as u32).map(|i| (i * 7 + 3) as u8).collect();
        f.send(&p)?;
        match f.recv(t)? {
            Some(r) => println!("len {len:>2}: got {:>2} bytes, prefix ok={}, padding zero={}", r.len(), r.starts_with(&p), r[p.len().min(r.len())..].iter().all(|&b| b == 0)),
            None => println!("len {len}: NO PACKET"),
        }
    }
    // max-size packet
    let max = f.tx_vacancy()? * 4;
    let p: Vec<u8> = (0..max).map(|i| i as u8).collect();
    f.send(&p)?;
    let r = f.recv(t)?.unwrap_or_default();
    println!("max {max} bytes: got {} bytes, ok={}", r.len(), r == p);
    // too large: must be refused, nothing written
    let before = (f.tx_vacancy()?, f.rx_occupancy()?);
    let big = vec![0u8; max + 1];
    match f.send(&big) {
        Err(FifoError::NoRoom { len, need, have }) => println!("oversize refused: len {len} need {need} have {have}"),
        other => println!("oversize NOT refused: {other:?}"),
    }
    println!("vacancy/occupancy before {before:?} after {:?}, stray packet: {:?}", (f.tx_vacancy()?, f.rx_occupancy()?), f.recv(Duration::from_millis(100))?);
    // ISR semantics
    f.clear_interrupts(0xFFFF_FFFF)?;
    println!("ISR after clear-all: {:#x}", f.interrupt_status()?);
    f.send(&[1, 2, 3, 4])?;
    std::thread::sleep(Duration::from_millis(5));
    let isr = f.interrupt_status()?;
    println!("ISR after one packet: {isr:#x}");
    f.clear_interrupts(0)?;
    println!("write-0 clears nothing: {}", f.interrupt_status()? == isr);
    f.clear_interrupts(isr)?;
    println!("write-1 clears: ISR now {:#x}", f.interrupt_status()?);
    let _ = f.recv(t)?;
    // interrupt-driven receive
    let irq = design_1::pac::axi_fifo_mm_s_0::IRQ_INTERRUPT.open()?;
    f.clear_interrupts(0xFFFF_FFFF)?;
    f.set_interrupt_mask(1 << 26)?; // receive complete
    for round in 0..3u8 {
        f.send(&[round; 12])?;
        let ev = irq.wait(Some(Duration::from_secs(1)))?;
        let isr = f.interrupt_status()?;
        let p = f.try_recv()?;
        f.clear_interrupts(isr)?;
        println!("round {round}: event {ev:?}, ISR {isr:#x}, packet {:?}", p.map(|p| p.len()));
    }
    println!("idle wait -> {:?}", irq.wait(Some(Duration::from_millis(300)))?);
    Ok(())
}
