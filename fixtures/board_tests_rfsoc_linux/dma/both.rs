//! Both DMAs of the loopback design (DMA 0 with 32-bit addresses, DMA 1 with 40-bit ones and 128-bit data), each
//! waiting for its interrupts, on `u-dma-buf` buffers: one large transfer each, then many short ones with new data
//! every time. Needs `/dev/udmabuf0` to `3` of at least 1 MiB and `interrupts.dtso` applied.
use design_1::{Design, pac};
use hardroxide_dma::{DmaBuffer, Udmabuf};
use hardroxide_ip::dma::Direction;
use std::time::{Duration, Instant};

const BYTES: usize = 1 << 20;
const ROUNDS: usize = 500;

fn pattern(seed: u32, out: &mut [u8]) {
    for (i, b) in out.iter_mut().enumerate() {
        *b = (i as u32)
            .wrapping_add(seed)
            .wrapping_mul(0x9E37_79B9)
            .rotate_left(9) as u8;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut d = Design::load()?;
    let irqs = [
        [
            pac::axi_dma_0::IRQ_MM2S_INTROUT.open()?,
            pac::axi_dma_0::IRQ_S2MM_INTROUT.open()?,
        ],
        [
            pac::axi_dma_1::IRQ_MM2S_INTROUT.open()?,
            pac::axi_dma_1::IRQ_S2MM_INTROUT.open()?,
        ],
    ];

    for (n, dma) in [&mut d.axi_dma_0, &mut d.axi_dma_1].into_iter().enumerate() {
        let mut source = Udmabuf::open(2 * n as u32)?;
        let mut destination = Udmabuf::open(2 * n as u32 + 1)?;
        dma.reset()?;
        dma.set_interrupts(true);
        let [sent, received] = &irqs[n];
        let mut expected = vec![0u8; BYTES];
        let (mut first, mut short) = (Duration::ZERO, Instant::now());
        for round in 0..=ROUNDS {
            // The first round is the large one; the others have lengths of whole 16-byte words up to 4 KiB.
            let len = if round == 0 {
                BYTES
            } else {
                16 * (1 + round * 37 % 256)
            };
            pattern((n * 100_000 + round) as u32, &mut expected[..len]);
            source.as_mut_slice()[..len].copy_from_slice(&expected[..len]);
            destination.as_mut_slice()[..len].fill(0);
            sent.enable()?;
            received.enable()?;
            let t0 = Instant::now();
            let receive = dma
                .start(Direction::StreamToMem, destination, len)
                .map_err(|e| e.error)?;
            let send = dma
                .start(Direction::MemToStream, source, len)
                .map_err(|e| e.error)?;
            for (irq, name) in [(sent, "send"), (received, "receive")] {
                let event = irq.wait_event(Some(Duration::from_secs(1)))?;
                assert!(event.is_some(), "DMA {n}, round {round}: no {name} interrupt");
            }
            if round == 0 {
                first = t0.elapsed();
                short = Instant::now();
            }
            source = send.wait(Duration::from_secs(1))?;
            destination = receive.wait(Duration::from_secs(1))?;
            assert!(
                destination.as_slice()[..len] == expected[..len],
                "DMA {n}, round {round}: data"
            );
        }
        println!(
            "DMA {n} at {:#x}: 1 MiB there and back in {} us; {ROUNDS} short transfers ({} ms): ok",
            destination.phys_addr(),
            first.as_micros(),
            short.elapsed().as_millis()
        );
    }
    println!("DMA OK");
    Ok(())
}
