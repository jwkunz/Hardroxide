# Tutorial 4: An HLS kernel

**Design:** `fixtures/pynq_z2_hls_vadd.xsa` · **Time:** 10 minutes · **Needs:**
[setup](index.md#before-you-start-once); the interrupt overlay for step 4

A Vitis HLS kernel is C++ compiled into FPGA logic. This one adds two numbers:

```cpp
{{#include ../../fixtures/hls/vadd.cpp}}
```

HLS gives the kernel an AXI-Lite control block: a start/done/idle register and one
register per argument. Hardroxide reads those register names from the design
and generates a typed wrapper, so you call `set_a`, `set_b`, `start`, and read
`out_r`. The kernel sits at `0x40000000` and its `interrupt` output reaches
`IRQ_F2P[0]`.

## 1. Inspect and generate

```console
$ hardroxide inspect fixtures/pynq_z2_hls_vadd.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: abaad84c...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x40000000      64 KiB  vadd_0  ->  crate::ips::Vadd0 (HLS)  (xilinx.com:hls:vadd:1.0)
                           irq interrupt -> IRQ_F2P[0] (GIC SPI 29)

1 instances: 0 with a curated driver, 1 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_hls_vadd.xsa -o work/vadd --hardroxide-path .
```

The kernel is marked `(HLS)`: it becomes `crate::ips::Vadd0`, defined in the
generated `src/ips.rs`. Open that file: you will find `set_a`, `set_b`, `out_r`
and `out_r_ctrl`, named after the kernel's registers in the design. The generic
control (`start`, `is_idle`, `is_done`, `wait_done`, interrupt control) comes from
the library's `HlsIp` and is available on the wrapper too.

## 2. Look before you start it

`hls_smoke` is generated for HLS designs. It prints the control bits and every
readable register, **without starting the kernel**, so it is safe to run on a
design you have not seen before:

```console
$ scripts/board/run.sh work/vadd hls_smoke
idle=true ready=false done=false
out_r = 0x0
out_r_ctrl = 0x0
```

`idle=true` means the kernel is waiting for a start.

## 3. Run it

```rust,ignore
{{#include ../../fixtures/board_tests/hls_vadd/vadd_run.rs}}
```

```console
$ cp fixtures/board_tests/hls_vadd/vadd_run.rs work/vadd/examples/
$ scripts/board/run.sh work/vadd vadd_run
1 + 2 = 3 (ctrl 0x1)
100 + 23 = 123 (ctrl 0x1)
4294967295 + 1 = 0 (ctrl 0x1)
7 + 35 = 42 (ctrl 0x1)
```

Four additions, including `0xFFFFFFFF + 1 = 0` (a 32-bit wrap). The `ctrl 0x1` is
the "output valid" bit HLS provides for each output register.

## 4. Wait for the done interrupt

Polling `wait_done` is fine for a few microseconds; for longer kernels let the
processor sleep. Apply this design's overlay
([Tutorial 2, step 4](02-timer-spi-irq.md#4-apply-the-overlay)), then:

```rust,ignore
{{#include ../../fixtures/board_tests/hls_vadd/vadd_irq.rs}}
```

```console
$ cp fixtures/board_tests/hls_vadd/vadd_irq.rs work/vadd/examples/
$ scripts/board/run.sh work/vadd vadd_irq
round 0: event Some(1) flags 0x1 out=10
round 1: event Some(2) flags 0x1 out=11
round 2: event Some(3) flags 0x1 out=12
```

The pattern is: enable the kernel's done interrupt once, then for each run
`start`, `wait` on the UIO line, and `ack_interrupts` (the line stays asserted
until you do). `flags 0x1` is the `ap_done` flag you just acknowledged.

## What you did

You ran HLS-generated hardware from Rust with named, typed registers, both by
polling and by interrupt. For your own kernel, the only change is the design you
point `generate` at.

Continue with [Tutorial 5: An HLS kernel that reads memory](05-hls-pointers.md).
