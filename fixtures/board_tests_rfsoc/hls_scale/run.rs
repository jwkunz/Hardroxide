//! Run the HLS `scale` kernel (`out[i] = in[i] * k`), which reads and writes DDR itself: the generated
//! `set_*_buffer` setters give it the buffers' addresses.
#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, pac, psu};
use hardroxide_dma::{DmaBuffer, static_dma_buffer};
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::time::Instant;
use psu::println;
use psu::wait::IrqWait;

psu::entry!(main);

const N: usize = 65536;

fn main() -> ! {
    let d = Design::init().expect("design");
    let s = &d.scale_0;
    let irq = IrqWait::for_line(&pac::scale_0::IRQ_INTERRUPT).expect("interrupt");
    d.enable_interrupts();
    let mut a = static_dma_buffer!(N * 4).unwrap();
    let mut b = static_dma_buffer!(N * 4).unwrap();
    s.enable_done_interrupt().unwrap();
    for k in [7i32, -3] {
        for (i, c) in a.as_mut_slice().chunks_exact_mut(4).enumerate() {
            c.copy_from_slice(&(i as i32 - 100).to_le_bytes());
        }
        b.as_mut_slice().fill(0xEE);
        s.set_in_r_buffer(&a).unwrap();
        s.set_out_r_buffer(&b).unwrap();
        s.set_n(N as u32).unwrap();
        s.set_k(k as u32).unwrap();
        irq.enable().unwrap();
        let t0 = Instant::now();
        s.start().unwrap();
        let event = irq.wait_event(Some(Duration::from_secs(1))).unwrap();
        let took = t0.elapsed().as_micros();
        assert!(event.is_some(), "no interrupt");
        s.ack_interrupts().unwrap();
        for (i, c) in b.as_slice().chunks_exact(4).enumerate() {
            let got = i32::from_le_bytes(c.try_into().unwrap());
            assert_eq!(got, (i as i32 - 100) * k, "element {i}");
        }
        println!("{N} elements times {k} in {took} us: ok");
    }
    println!("HLS SCALE OK");
    design_1::halt()
}
