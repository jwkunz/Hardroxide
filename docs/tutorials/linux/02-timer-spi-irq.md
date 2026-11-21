# Linux 2: Timer, SPI and interrupts

**Design:** `fixtures/pynq_z2_peripherals.xsa` · **Time:** 15 minutes · **Needs:**
[setup](index.md#before-you-start-once); the interrupt overlay (step 4)

Three peripherals the board can run with no wiring: an **AXI Timer**, an
**AXI Quad SPI** (its internal loopback mode), and an **AXI IIC**. Their three
interrupt outputs are combined and reach the processor as `IRQ_F2P[0]`
(Timer), `[1]` (IIC) and `[2]` (SPI). The IIC and SPI pins are not constrained, so
nothing is wired out of the chip.

You will learn how an interrupt reaches your program: the generator writes a
device-tree overlay, you apply it on the board, and your code sleeps on a
`/dev/uioN` file until the hardware fires.

## 1. Inspect and generate

```console
$ hardroxide inspect fixtures/pynq_z2_peripherals.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: 5ffd7edd...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x41600000      64 KiB  axi_iic_0  ->  hardroxide_ip::iic::AxiIic  (xilinx.com:ip:axi_iic:2.1)
                           irq iic2intc_irpt -> IRQ_F2P[1] (GIC SPI 30)
0x41e00000      64 KiB  axi_quad_spi_0  ->  hardroxide_ip::spi::AxiSpi  (xilinx.com:ip:axi_quad_spi:3.2)
                           irq ip2intc_irpt -> IRQ_F2P[2] (GIC SPI 31)
0x42800000      64 KiB  axi_timer_0  ->  hardroxide_ip::timer::AxiTimer  (xilinx.com:ip:axi_timer:2.0)
                           irq interrupt -> IRQ_F2P[0] (GIC SPI 29)

3 instances: 3 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_peripherals.xsa -o work/peripherals --hardroxide-path .
```

The `irq` lines tell you where each interrupt goes: `IRQ_F2P[n]` is the bit of
the PL-to-PS interrupt vector, `GIC SPI 29` is the interrupt number Linux sees.
Three fields appear on `Design`: `axi_timer_0`, `axi_iic_0` and `axi_quad_spi_0`.

## 2. Polling: no overlay needed

The generated examples that only poll registers work straight away:

```console
$ scripts/board/run.sh work/peripherals timer_delay
100000000 Hz timer: asked 100 ms, took 100.010449ms
$ scripts/board/run.sh work/peripherals spi_loopback
SPI loopback OK: 40 bytes
$ scripts/board/run.sh work/peripherals i2c_scan
```

- `timer_delay` asks the hardware timer for a 100 ms delay and measures it
  against the system clock. The extra ~20 µs is Linux scheduling.
- `spi_loopback` sets the SPI core's *internal loopback* bit, sends 40 bytes and
  checks they come back: it exercises the real core without a wire.
- `i2c_scan` probes every address on the I2C bus. Nothing is connected, so it
  finds nothing and prints nothing. That is the correct output here; the next
  tutorial gives the IIC a device to find.

## 3. How an interrupt reaches Linux

Polling burns CPU. To *sleep* until the hardware signals, Linux needs to know the
interrupt exists. Hardroxide generates `interrupts.dtso`, a device-tree overlay
with one `generic-uio` node per interrupt:

```console
$ cat work/peripherals/interrupts.dtso | head -24
```

Each node says "an interrupt at this address, wired to this GIC input, and expose
it as a UIO device". Once applied, `/dev/uio0`, `uio1`, ... exist, and reading one
blocks until its interrupt fires.

## 4. Apply the overlay

```console
$ scp work/peripherals/interrupts.dtso $BOARD:
$ ssh -t $BOARD sudo sh overlay.sh interrupts.dtso
overlay status: applied
uio0: axi_iic_0_iic2intc_irpt
uio1: axi_quad_spi_0_ip2intc_irpt
uio2: axi_timer_0_interrupt
```

Now `board_check` can prove every interrupt has a device:

```console
$ scripts/board/run.sh work/peripherals board_check
1. loading bitstream and applying clocks ...
   ok
2. FCLK readback (SLCR) vs. the design:
   FCLK0: 100000000 Hz (want 100000000) ok
3. interrupt UIO devices (needs interrupts.dtso applied):
   axi_iic_0.iic2intc_irpt: ok
   axi_quad_spi_0.ip2intc_irpt: ok
   axi_timer_0.interrupt: ok
all checks passed; next run the per-IP examples
```

> **One overlay at a time.** `overlay.sh` replaces the overlay it applied before.
> Also, an interrupt line keeps the trigger type (level or edge) of the first
> overlay that used it until the board reboots; if a later design needs the other
> type on the same line, its UIO device returns `EIO` and you must reboot. See
> [Interrupts](../../guide/interrupts.md).

## 5. Wait for the interrupt

This program starts the timer as a one-shot with its interrupt enabled, then
blocks on the timer's UIO device. Compare it with `timer_delay`, which spins:

```rust,ignore
{{#include ../../../fixtures/board_tests/peripherals/timer_irq.rs}}
```

- `IRQ_INTERRUPT.open()` is a constant the generator wrote into `pac`; it finds
  the right `/dev/uioN` by name.
- `irq.wait(Some(timeout))` unmasks the interrupt and sleeps; it returns the
  interrupt's cumulative event count, or `None` if the timeout expires.
- The count goes up by one per event (you see `1`, `2`, `3`); that is how you
  can tell you did not miss one.

```console
$ cp fixtures/board_tests/peripherals/timer_irq.rs work/peripherals/examples/
$ scripts/board/run.sh work/peripherals timer_irq
round 0: event Some(1) after 50.073766ms, expired=true
round 1: event Some(2) after 50.075495ms, expired=true
round 2: event Some(3) after 50.068732ms, expired=true
```

50 ms asked, about 50.08 ms observed, with no polling.

## What you did

You ran polling examples, applied an interrupt overlay, and waited for a hardware
interrupt from a normal Rust program. The same pattern (open the line from `pac`,
`wait`, acknowledge in the device) works for every IP with an interrupt.

**Try next:** the same crate has `uio_backend.rs`, which drives the timer through
its `/dev/uioN` instead of `/dev/mem`; `lifecycle.rs`, which loads the design
repeatedly; and `async_bench.rs`, which waits asynchronously (it needs the `async`
feature of `hardroxide-irq`). All are in `fixtures/board_tests/peripherals/`.

Continue with [Linux 3: UART and I2C without wires](03-uart-iic-loopback.md).
