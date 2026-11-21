//! Bare metal: loop data through the AXI DMA (MM2S to S2MM over the design's stream loopback),
//! check every transfer and report the throughput. Bare metal tutorial 3.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_dma::{DmaBuffer, static_dma_buffer};
use hardroxide_ip::dma::Direction;
use hardroxide_mmio::time::Instant;
use ps7::println;

ps7::entry!(main);

const LEN: usize = 16 * 1024;
const ROUNDS: usize = 100;

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let dma = &d.axi_dma_0;
    dma.reset().unwrap();
    // Buffers in the `.dma` section: the MMU maps it non-cacheable, so the CPU and the DMA
    // engine see the same bytes with no cache maintenance.
    let mut tx = static_dma_buffer!(LEN).unwrap();
    let mut rx = static_dma_buffer!(LEN).unwrap();

    println!("\n== dma_speed: {ROUNDS} loopbacks of {LEN} bytes ==");
    let t0 = Instant::now();
    let mut dma_us = 0u64;
    let mut bad = 0;
    for round in 0..ROUNDS {
        for (i, b) in tx.as_mut_slice().iter_mut().enumerate() {
            *b = (i + round * 31) as u8;
        }
        // Arm the receiver first, then send; `wait` hands each buffer back.
        let t1 = Instant::now();
        let rx_t = dma.start(Direction::StreamToMem, rx, LEN).map_err(|e| e.error).unwrap();
        let tx_t = dma.start(Direction::MemToStream, tx, LEN).map_err(|e| e.error).unwrap();
        tx = tx_t.wait(Duration::from_secs(1)).unwrap();
        rx = rx_t.wait(Duration::from_secs(1)).unwrap();
        dma_us += t1.elapsed().as_micros() as u64;
        if tx.as_slice() != rx.as_slice() {
            bad += 1;
        }
    }
    let us = t0.elapsed().as_micros() as u64;
    let bytes = (LEN * ROUNDS) as u64;
    println!("{bad} mismatches; {bytes} bytes");
    println!("DMA only:   {dma_us} us = {} MB/s", bytes / dma_us);
    println!("whole loop: {us} us = {} MB/s (the CPU fills and compares the buffers one byte at a time)", bytes / us);
    halt()
}
