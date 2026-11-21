# Bare metal 3: DMA with the caches on

**Design:** `fixtures/board/pynq_z2_dma_loopback.xsa` · **Time:** 15 minutes · **Needs:** [Bare metal 1](01-jtag.md)

On Linux, DMA buffers come from the `u-dma-buf` kernel module ([Linux 6](../linux/06-dma.md)). Bare metal has no kernel to ask,
and it has a trap Linux hides: the processor's caches. `Design::init()` turns on the MMU and the L1 and L2 caches, which makes
ordinary code about 20 times faster, but a DMA engine reads and writes DDR directly and cannot see what is still in a cache.
Hardroxide's answer is a linker section, `.dma`, that the MMU maps **non-cacheable**: a buffer placed there is always the same
bytes to the processor and the DMA engine, with no cache maintenance to forget.

## 1. Generate and run the example

```console
$ hardroxide generate fixtures/board/pynq_z2_dma_loopback.xsa --platform baremetal -o work/bmd --hardroxide-path .
generated 10 files in work/bmd
$ cd work/bmd && cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/dma_loopback --xsa ../../fixtures/board/pynq_z2_dma_loopback.xsa --watch 8
loading target/armv7a-none-eabihf/release/examples/dma_loopback ...
running; console below (8 s) ...
DMA loopback OK: 4096 bytes
```

The design sends the DMA's output stream straight back into its input, so what the processor sends must come back unchanged.

## 2. The buffers

```rust,ignore
let mut tx = static_dma_buffer!(4096).unwrap();   // lives in .dma: non-cacheable
let rx = static_dma_buffer!(4096).unwrap();
let rx_t = dma.start(Direction::StreamToMem, rx, n).map_err(|e| e.error).unwrap();   // arm the receiver first
let tx_t = dma.start(Direction::MemToStream, tx, n).map_err(|e| e.error).unwrap();
let tx = tx_t.wait(Duration::from_secs(1)).unwrap();                                 // buffers come back
let rx = rx_t.wait(Duration::from_secs(1)).unwrap();
```

`static_dma_buffer!` can be used once per call site (it makes a static, and returns `None` the second time). `start` takes the
buffer by value, so the compiler stops you touching it while the engine owns it; `wait` gives it back.

## 3. Your own program: how fast, and what does the processor cost?

Copy `fixtures/board_tests_bm/baremetal_dma/dma_speed.rs` into `examples/`. It repeats a 16 KiB loopback 100 times, checks every
transfer, and times the DMA separately from the loop as a whole:

```console
$ cargo build --release --example dma_speed
$ hardroxide run target/armv7a-none-eabihf/release/examples/dma_speed --xsa ../../fixtures/board/pynq_z2_dma_loopback.xsa --watch 8
running; console below (8 s) ...

== dma_speed: 100 loopbacks of 16384 bytes ==
0 mismatches; 1638400 bytes
DMA only:   4742 us = 345 MB/s
whole loop: 331289 us = 4 MB/s (the CPU fills and compares the buffers one byte at a time)
```

The DMA engine moves 345 MB/s, close to the limit of this design's 32-bit stream at 100 MHz (400 MB/s). The whole loop runs at
4 MB/s because the *processor* is filling and comparing the buffers byte by byte through non-cacheable memory, where every
access goes to DDR. That is the price of the simple scheme, and the reason the next step up (cached buffers with explicit
`clean_range` before a transfer and `invalidate_range` after one, see [`hardroxide-ps7`](../../reference/ps7.md#memory-mmu-caches-l2-mem))
exists. Use `.dma` buffers when the processor touches the data lightly or in words; reach for cache maintenance when it
processes every byte.

Next: the [RTOS tutorial](../rtos/01-control-loop.md), or `hardroxide package` to start these programs [without the
cable](../../guide/baremetal.md#booting-on-its-own).
