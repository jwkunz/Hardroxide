# Tutorial 6: AXI DMA

**Design:** `fixtures/pynq_z2_dma_loopback.xsa` · **Time:** 15 minutes · **Needs:**
[setup](index.md#before-you-start-once), `u-dma-buf` (see
[Tutorial 5](05-hls-pointers.md#1-get-u-dma-buf)), the interrupt overlay for step 4

An **AXI DMA** engine moves data between memory and an AXI4-Stream port by itself.
In this design its stream output (MM2S: memory to stream) is looped back into its
stream input (S2MM: stream to memory) through a FIFO, so whatever you send comes
straight back and you can check it. The DMA is at `0x40400000`; its two interrupts
are `IRQ_F2P[0]` (MM2S) and `[1]` (S2MM).

```console
$ hardroxide inspect fixtures/pynq_z2_dma_loopback.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: 7a179617...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x40400000      64 KiB  axi_dma_0  ->  hardroxide_ip::dma::AxiDma  (xilinx.com:ip:axi_dma:7.1)
                           irq mm2s_introut -> IRQ_F2P[0] (GIC SPI 29)
                           irq s2mm_introut -> IRQ_F2P[1] (GIC SPI 30)

1 instances: 1 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_dma_loopback.xsa -o work/dma --hardroxide-path .
```

## 1. The generated check

```console
$ scripts/board/run.sh work/dma dma_loopback
DMA loopback OK: 4096 bytes
```

Open `work/dma/examples/dma_loopback.rs`. The important idea is **ownership**: starting a
transfer *moves the buffer into the transfer*, and you only get it back when the
hardware is finished.

```rust,ignore
let rx_t = dma.start(Direction::StreamToMem, rx, n)?;   // `rx` is gone: the hardware owns it
let tx_t = dma.start(Direction::MemToStream, tx, n)?;
let tx = tx_t.wait(Duration::from_secs(1))?;            // ... and comes back here
let rx = rx_t.wait(Duration::from_secs(1))?;
```

Because the buffer is *moved*, the compiler stops you from touching memory the DMA
is still using. Arm the receiver first, then start the sender.

## 2. Interrupt-driven transfers

Instead of spinning in `wait`, sleep on the DMA's interrupts. Apply the overlay
([Tutorial 2](02-timer-spi-irq.md#4-apply-the-overlay)), then:

```rust,ignore
{{#include ../../fixtures/board_tests/dma_loopback/dma_irq.rs}}
```

```console
$ cp fixtures/board_tests/dma_loopback/dma_irq.rs work/dma/examples/
$ scripts/board/run.sh work/dma dma_irq
round 0: irq rx Some(1) tx Some(2), data ok = true
round 1: irq rx Some(3) tx Some(4), data ok = true
round 2: irq rx Some(5) tx Some(6), data ok = true
```

`set_interrupts(true)` turns on the DMA's completion and error interrupts; after an
interrupt, `wait(Duration::ZERO)` collects the result and clears the status flags.
The `Some(n)` values are cumulative event counts of each line.

## 3. Robustness and speed

`dma_edge` checks the awkward cases: a transfer that can never complete (S2MM with
nothing sending) times out and leaves the engine usable; lengths from 4 bytes to
1 MiB; throughput; and 200 transfers back to back.

```console
$ cp fixtures/board_tests/dma_loopback/dma_edge.rs work/dma/examples/
$ scripts/board/run.sh work/dma dma_edge
max transfer length: 8388607 bytes
buffers: tx 1048576 bytes @ 0x15900000, rx 1048576 bytes @ 0x15a00000
S2MM with no sender: Some(Timeout(100ms)) after 200.112689ms
recovery loopback ok: true
       4 bytes: ok=true in 6.422µs (0.6 MB/s)
       8 bytes: ok=true in 5.635µs (1.4 MB/s)
      60 bytes: ok=true in 5.573µs (10.8 MB/s)
    1000 bytes: ok=true in 5.893µs (169.7 MB/s)
    4096 bytes: ok=true in 12.032µs (340.4 MB/s)
   65536 bytes: ok=true in 166.68µs (393.2 MB/s)
 1048576 bytes: ok=true in 2.621824ms (399.9 MB/s)
200 x 4096-byte loopbacks: 0 mismatches, 113.396931ms
```

About 400 MB/s is what a 32-bit stream at 100 MHz can carry, so the DMA is running at
the limit of the design. (The first test timing out after 200 ms is the intended
behaviour: the driver soft-resets the engine to recover.)

## What you did

You moved data with a DMA engine under compiler-enforced buffer ownership, by
polling and by interrupt, and measured the throughput. For your own design,
connect the stream ports to your logic instead of back to each other.

Continue with [Tutorial 7: Many interrupts, one controller](07-interrupt-controller.md).
