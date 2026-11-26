//! Both DMAs of the loopback design, each waiting for its interrupts: DMA 0 (32-bit addresses) with buffers in the
//! non-cacheable `.dma` section, DMA 1 (40-bit addresses, 128-bit data) with cached buffers above 4 GiB and explicit
//! cache maintenance.
#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, pac, psu};
use hardroxide_dma::{DmaBuffer, StaticBuffer, static_dma_buffer};
use hardroxide_ip::dma::Direction;
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::time::Instant;
use psu::println;
use psu::wait::IrqWait;

psu::entry!(main);

const BYTES: usize = 1 << 20;
/// In the DDR above 4 GiB: nothing else of the program is there.
const HIGH_SOURCE: usize = 0x8_4000_0000;
const HIGH_DESTINATION: usize = 0x8_4100_0000;

fn pattern(seed: u32, out: &mut [u8]) {
    for (i, b) in out.iter_mut().enumerate() {
        *b = (i as u32)
            .wrapping_add(seed)
            .wrapping_mul(0x9E37_79B9)
            .rotate_left(9) as u8;
    }
}

fn main() -> ! {
    let mut d = Design::init().expect("design");
    let irqs = [
        [
            &pac::axi_dma_0::IRQ_MM2S_INTROUT,
            &pac::axi_dma_0::IRQ_S2MM_INTROUT,
        ],
        [
            &pac::axi_dma_1::IRQ_MM2S_INTROUT,
            &pac::axi_dma_1::IRQ_S2MM_INTROUT,
        ],
    ]
    .map(|pair| pair.map(|l| IrqWait::for_line(l).expect("interrupt")));
    d.enable_interrupts();

    let mut low_source = static_dma_buffer!(BYTES).unwrap();
    let low_destination = static_dma_buffer!(BYTES).unwrap();
    // SAFETY: DDR that only these two buffers use, for the program's whole run.
    let (mut high_source, high_destination) = unsafe {
        (
            StaticBuffer::identity(core::slice::from_raw_parts_mut(
                HIGH_SOURCE as *mut u8,
                BYTES,
            )),
            StaticBuffer::identity(core::slice::from_raw_parts_mut(
                HIGH_DESTINATION as *mut u8,
                BYTES,
            )),
        )
    };
    pattern(1, low_source.as_mut_slice());
    pattern(2, high_source.as_mut_slice());

    for (n, source, destination, cached) in [
        (0, low_source, low_destination, false),
        (1, high_source, high_destination, true),
    ] {
        let dma = if n == 0 {
            &mut d.axi_dma_0
        } else {
            &mut d.axi_dma_1
        };
        dma.reset().unwrap();
        dma.set_interrupts(true);
        if cached {
            // What the processor wrote must be in memory before the DMA reads it, and no line of the destination may
            // be written back over what the DMA puts there.
            psu::mem::clean_range(source.phys_addr() as usize, BYTES);
            psu::mem::flush_range(destination.phys_addr() as usize, BYTES);
        }
        let [sent, received] = &irqs[n];
        sent.enable().unwrap();
        received.enable().unwrap();
        let t0 = Instant::now();
        let receive = dma
            .start(Direction::StreamToMem, destination, BYTES)
            .map_err(|e| e.error)
            .unwrap();
        let send = dma
            .start(Direction::MemToStream, source, BYTES)
            .map_err(|e| e.error)
            .unwrap();
        for (irq, name) in [(sent, "send"), (received, "receive")] {
            let event = irq.wait_event(Some(Duration::from_secs(1))).unwrap();
            assert!(event.is_some(), "DMA {n}: no {name} interrupt");
        }
        let took = t0.elapsed().as_micros().max(1);
        let source = send.wait(Duration::from_secs(1)).unwrap();
        let destination = receive.wait(Duration::from_secs(1)).unwrap();
        if cached {
            psu::mem::invalidate_range(destination.phys_addr() as usize, BYTES);
        }
        assert!(source.as_slice() == destination.as_slice(), "DMA {n}: data");
        println!(
            "DMA {n} at {:#x}: 1 MiB there and back in {took} us, both interrupts: ok",
            destination.phys_addr()
        );
    }
    println!("DMA OK");
    design_1::halt()
}
