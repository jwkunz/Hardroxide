# Linux 9: AXI4-Stream FIFO

**Design:** `fixtures/pynq_z2_fifo.xsa` · **Time:** 10 minutes · **Needs:**
[setup](index.md#before-you-start-once); the interrupt overlay for step 3

The **AXI4-Stream FIFO** lets software send and receive *packets* on an AXI4-Stream
port through registers, no DMA needed. In this design the FIFO's stream output is
connected back to its own stream input, so every packet you send comes back. It is at
`0x43c00000`, interrupt on `IRQ_F2P[0]`.

```console
$ hardroxide inspect fixtures/pynq_z2_fifo.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: abad49da...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x43c00000      64 KiB  axi_fifo_mm_s_0  ->  hardroxide_ip::fifo::AxiFifo  (xilinx.com:ip:axi_fifo_mm_s:4.3)
                           irq interrupt -> IRQ_F2P[0] (GIC SPI 29)

1 instances: 1 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_fifo.xsa -o work/fifo --hardroxide-path .
```

## 1. Send a packet and get it back

```console
$ scripts/board/run.sh work/fifo fifo_loopback
loopback ok: sent 37 bytes, core reported 40 (rounded up to whole words)
```

You sent 37 bytes and got **40** back. That is not a bug: the core moves 32-bit
words and reports a received packet's length in whole words (it does not carry
byte enables here). The extra three bytes are zero. If you need an exact
length, carry it in the payload.

## 2. The edge cases

```rust,ignore
{{#include ../../../fixtures/board_tests/fifo/fifo_edge.rs}}
```

```console
$ cp fixtures/board_tests/fifo/fifo_edge.rs work/fifo/examples/
$ scp work/fifo/interrupts.dtso $BOARD:
$ ssh -t $BOARD sudo sh overlay.sh interrupts.dtso
$ scripts/board/run.sh work/fifo fifo_edge
tx vacancy 508 words, rx occupancy 0
len  1: got  4 bytes, prefix ok=true, padding zero=true
len  2: got  4 bytes, prefix ok=true, padding zero=true
len  3: got  4 bytes, prefix ok=true, padding zero=true
len  4: got  4 bytes, prefix ok=true, padding zero=true
len  5: got  8 bytes, prefix ok=true, padding zero=true
len  7: got  8 bytes, prefix ok=true, padding zero=true
len  8: got  8 bytes, prefix ok=true, padding zero=true
len  9: got 12 bytes, prefix ok=true, padding zero=true
len 63: got 64 bytes, prefix ok=true, padding zero=true
len 64: got 64 bytes, prefix ok=true, padding zero=true
len 65: got 68 bytes, prefix ok=true, padding zero=true
max 2032 bytes: got 2032 bytes, ok=true
oversize refused: len 2033 need 509 have 508
vacancy/occupancy before (508, 0) after (508, 0), stray packet: None
ISR after clear-all: 0x0
ISR after one packet: 0xc000000
write-0 clears nothing: true
write-1 clears: ISR now 0x0
round 0: event Some(1), ISR 0xc000000, packet Some(12)
round 1: event Some(2), ISR 0xc000000, packet Some(12)
round 2: event Some(3), ISR 0xc000000, packet Some(12)
idle wait -> None
```

Things to notice:

- Lengths round up to a whole word (1, 2, 3 and 4 bytes all come back as 4);
- the largest packet is the FIFO's vacancy: 508 words = 2032 bytes;
- a packet that does not fit is **refused before anything is written**
  (`NoRoom`), so the FIFO is never left with half a packet;
- the status flags (`ISR`) clear when you write 1s to them, and writing 0 does
  nothing; `0x0c000000` is "transmit complete" plus "receive complete";
- the receive interrupt fires once per packet and re-arms.

## What you did

You moved packets through a stream port from ordinary register accesses, learned
the word-granularity rule, and used the receive interrupt.

Continue with [Linux 10: Updating a bitstream](10-update.md).
