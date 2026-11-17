//! Interrupt-driven AXI DMA: start the S2MM and MM2S channels, sleep on their
//! interrupts, then collect the buffers. Needs `interrupts.dtso` applied and two
//! `u-dma-buf` devices.
use hardroxide_dma::{DmaBuffer, Udmabuf};
use hardroxide_ip::dma::Direction;
use std::time::Duration;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut d = design_1::Design::load()?;
    d.axi_dma_0.reset()?;
    d.axi_dma_0.set_interrupts(true);
    let dma = &d.axi_dma_0;
    let tx_irq = design_1::pac::axi_dma_0::IRQ_MM2S_INTROUT.open()?;
    let rx_irq = design_1::pac::axi_dma_0::IRQ_S2MM_INTROUT.open()?;
    let mut tx = Udmabuf::open(0)?;
    let mut rx = Udmabuf::open(1)?;
    let n = 4096.min(tx.len()).min(rx.len());
    for round in 0..3u8 {
        for (i, b) in tx.as_mut_slice()[..n].iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(3).wrapping_add(round);
        }
        rx.as_mut_slice()[..n].fill(0);
        let rx_t = dma
            .start(Direction::StreamToMem, rx, n)
            .map_err(|e| e.error)?;
        let tx_t = dma
            .start(Direction::MemToStream, tx, n)
            .map_err(|e| e.error)?;
        let a = rx_irq.wait(Some(Duration::from_secs(2)))?;
        let b = tx_irq.wait(Some(Duration::from_secs(2)))?;
        tx = tx_t.wait(Duration::ZERO)?;
        rx = rx_t.wait(Duration::ZERO)?;
        println!(
            "round {round}: irq rx {a:?} tx {b:?}, data ok = {}",
            tx.as_slice()[..n] == rx.as_slice()[..n]
        );
    }
    Ok(())
}
