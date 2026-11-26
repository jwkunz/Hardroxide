//! Run the HLS `scale` kernel (`out[i] = in[i] * k`), which reads and writes DDR itself: the generated
//! `set_*_buffer` setters give it the addresses of two `u-dma-buf` buffers (`/dev/udmabuf0` and `1`, at least 256 KiB).
use design_1::{Design, pac};
use hardroxide_dma::{DmaBuffer, Udmabuf};
use std::time::{Duration, Instant};

const N: usize = 65536;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = Design::load()?;
    let s = &d.scale_0;
    let irq = pac::scale_0::IRQ_INTERRUPT.open()?;
    let mut a = Udmabuf::open(0)?;
    let mut b = Udmabuf::open(1)?;
    s.enable_done_interrupt()?;
    for k in [7i32, -3, 11, -13] {
        for (i, c) in a.as_mut_slice()[..N * 4].chunks_exact_mut(4).enumerate() {
            c.copy_from_slice(&(i as i32 - 100 + k).to_le_bytes());
        }
        b.as_mut_slice()[..N * 4].fill(0xEE);
        s.set_in_r_buffer(&a)?;
        s.set_out_r_buffer(&b)?;
        s.set_n(N as u32)?;
        s.set_k(k as u32)?;
        irq.enable()?;
        let t0 = Instant::now();
        s.start()?;
        let event = irq.wait_event(Some(Duration::from_secs(1)))?;
        let took = t0.elapsed().as_micros();
        assert!(event.is_some(), "no interrupt");
        s.ack_interrupts()?;
        for (i, c) in b.as_slice()[..N * 4].chunks_exact(4).enumerate() {
            let got = i32::from_le_bytes(c.try_into()?);
            assert_eq!(got, (i as i32 - 100 + k) * k, "element {i}");
        }
        println!("{N} elements times {k} in {took} us: ok");
    }
    println!("HLS SCALE OK");
    Ok(())
}
