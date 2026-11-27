# Your own design 4: An HLS kernel that reads memory

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** Linux

**Design:** `fixtures/pynq_z2_hls_scale.xsa` · **Time:** 15 minutes · **Needs:**
[setup](../linux/index.md#before-you-start-once) and the `u-dma-buf` module

Real kernels work on arrays in memory. This one multiplies every element of an input
array by a constant and writes an output array:

```cpp
{{#include ../../../fixtures/hls_scale/scale.cpp}}
```

The pointer arguments `in` and `out` become **64-bit address registers** and the kernel
reads and writes memory itself, through the processor's `S_AXI_HP0` port. For that
to work, the arrays must be in *physically contiguous* memory whose address you
can hand to the hardware. On Linux that is the job of the `u-dma-buf` kernel
module: each `/dev/udmabufN` is a contiguous buffer mapped **uncached**, so you never
flush a cache.

## 1. Get `u-dma-buf`

The stock image does not include it. Build it once for your kernel
([Building u-dma-buf](../../guide/hardware.md#building-u-dma-buf)), copy the `.ko` to the
board, and load two 1 MiB buffers:

```console
$ ssh -t $BOARD sudo insmod u-dma-buf.ko udmabuf0=1048576 udmabuf1=1048576
$ ssh $BOARD ls /dev/udmabuf0 /dev/udmabuf1
/dev/udmabuf0  /dev/udmabuf1
```

## 2. Inspect and generate

```console
$ hardroxide inspect fixtures/pynq_z2_hls_scale.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: 21c275ac...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x40000000      64 KiB  scale_0  ->  crate::ips::Scale0 (HLS)  (xilinx.com:hls:scale:1.0)
                           irq interrupt -> IRQ_F2P[0] (GIC SPI 29)

1 instances: 0 with a curated driver, 1 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_hls_scale.xsa -o work/scale --hardroxide-path .
```

Besides `set_n` and `set_k`, the generated wrapper has, for each pointer argument,
`set_in_r_addr(u64)` and **`set_in_r_buffer(&buffer)`**: the second takes any DMA
buffer and writes its physical address for you. (`in_r` is the name HLS gives the
`in` argument; `in` is a reserved word in Verilog.)

## 3. Check the kernel is idle

```console
$ scripts/board/run.sh work/scale hls_smoke
idle=true ready=false done=false
```

## 4. Run it

```rust,ignore
{{#include ../../../fixtures/board_tests/hls_scale/scale_run.rs}}
```

The program fills the input buffer with `i - 100`, fills the output buffer with `0xEE` so a
kernel that does nothing is obvious, hands both buffers to the kernel, starts it,
and checks every element:

```console
$ cp fixtures/board_tests/hls_scale/scale_run.rs work/scale/examples/
$ scripts/board/run.sh work/scale scale_run
scale: 256 elements, 0 mismatches
```

## Why uncached buffers matter

If a CPU cache held part of the output while the FPGA wrote around it, you would read
stale data. `Udmabuf` maps the buffer uncached, so reads and writes always go to
memory, and no cache maintenance is needed. The price is that CPU access to the
buffer is slower; for large arrays process them in the FPGA, not in a CPU loop.

## What you did

You gave an HLS kernel pointers to real memory, ran it, and verified the result
element by element. The same `set_*_buffer` calls work for any number of pointer
arguments.

> **If results suddenly look wrong** (the last 16 words of a write missing, a
> `Timeout` that was not there before) right after switching between designs that use
> memory, see the known issue
> [G17](../../08-verification-gaps.md#g17-the-ps-pl-hp-port-can-wedge-after-many-design-switches):
> power-cycle the board.

Continue with [AXI DMA](05-dma.md).
