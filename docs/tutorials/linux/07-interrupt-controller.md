# Linux 7: Many interrupts, one controller

**Design:** `fixtures/pynq_z2_intc.xsa` · **Time:** 10 minutes · **Needs:**
[setup](index.md#before-you-start-once); the interrupt overlay

The processor has only a few PL interrupt inputs. Designs with many interrupt
sources combine them with an **AXI Interrupt Controller**, whose single output is
the one line that reaches the processor. In this design the Timer (input 0), the IIC
(input 1) and the GPIO (input 2) feed `axi_intc_0`, which drives `IRQ_F2P[0]`.

```console
$ hardroxide inspect fixtures/pynq_z2_intc.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: ec640a64...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x41200000      64 KiB  axi_gpio_0  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 2 of axi_intc_0
0x41600000      64 KiB  axi_iic_0  ->  hardroxide_ip::iic::AxiIic  (xilinx.com:ip:axi_iic:2.1)
                           irq iic2intc_irpt -> input 1 of axi_intc_0
0x41800000      64 KiB  axi_intc_0  ->  hardroxide_ip::intc::AxiIntc  (xilinx.com:ip:axi_intc:4.1)
                           irq irq -> IRQ_F2P[0] (GIC SPI 29)
0x42800000      64 KiB  axi_timer_0  ->  hardroxide_ip::timer::AxiTimer  (xilinx.com:ip:axi_timer:2.0)
                           irq interrupt -> input 0 of axi_intc_0

4 instances: 4 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_intc.xsa -o work/intc --hardroxide-path .
```

Read the `irq` lines: Hardroxide traced each source through the controller. Only
`axi_intc_0`'s own output has a GIC number; the others say `input N of axi_intc_0`.
That has consequences:

- `interrupts.dtso` contains **one** UIO node, the controller's;
- the Timer's own interrupt constant, `pac::axi_timer_0::IRQ_INTERRUPT`, has no UIO
  device. `IrqLine::open()` on it returns `IrqError::ViaController`, which tells you
  to wait on the controller instead.

## 1. Check the design

```console
$ scp work/intc/interrupts.dtso $BOARD: && ssh -t $BOARD sudo sh overlay.sh interrupts.dtso
$ scripts/board/run.sh work/intc board_check
1. loading bitstream and applying clocks ...
   ok
2. FCLK readback (SLCR) vs. the design:
   FCLK0: 100000000 Hz (want 100000000) ok
3. interrupt UIO devices (needs interrupts.dtso applied):
   axi_intc_0.irq: ok
all checks passed; next run the per-IP examples
$ scripts/board/run.sh work/intc timer_delay
100000000 Hz timer: asked 100 ms, took 100.022435ms
```

`board_check` skips lines that go through the controller (it only opens the
controller's own); the Timer still works as usual.

## 2. Wait on the controller

```rust,ignore
{{#include ../../../fixtures/board_tests/intc/intc_timer.rs}}
```

The sequence for any source behind a controller is:

1. `c.init()` and `c.enable(mask)`: switch on the inputs you care about;
2. start the device (`t.start(..., interrupt=true)`);
3. `c.wait(&irq, mask, timeout)`: sleeps on the controller's UIO line and returns
   the **pending bits** within `mask` (here `Some(1)` = input 0);
4. clear the device (`t.clear()`), **then** `c.ack(mask)`, in that order, or the
   source re-asserts at once.

```console
$ cp fixtures/board_tests/intc/intc_timer.rs work/intc/examples/
$ scripts/board/run.sh work/intc intc_timer
enabled 0x1
round 0: pending Some(1) after 30.086275ms
  after ack pending=0x0
round 1: pending Some(1) after 30.079126ms
  after ack pending=0x0
round 2: pending Some(1) after 30.073552ms
  after ack pending=0x0
idle wait -> None
```

`pending=0x0` after the acknowledge, and the final wait returns `None` because
nothing else is asserted.

## What you did

You handled an interrupt that reaches Linux only through a controller: find the
input bit, enable it, wait on the controller, then acknowledge at the source and at
the controller. With several devices, the returned bit mask tells you which one
fired.

Continue with [Linux 8: Block RAM and UART](08-bram-uart.md).
