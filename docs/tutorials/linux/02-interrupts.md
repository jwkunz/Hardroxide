# Linux 2: Interrupts: wait for a button

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 15 minutes · **Needs:** [Linux 1](01-lights.md)

[Tutorial 1](01-lights.md)'s program asked the buttons their state fifty times a second. An **interrupt** is the other
way round: the program sleeps, and the hardware wakes it when something happens.

> **Status:** new in 3.0: the program builds against the generated crate and has **not yet been run on a board**.

## 1. Where the interrupts go

```console
$ hardroxide inspect fixtures/board/pynq_base_overlay.xsa
```

```text
0x41210000      64 KiB  btns_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 13 of system_interrupts
0x41800000      64 KiB  system_interrupts  ->  hardroxide_ip::intc::AxiIntc  (xilinx.com:ip:axi_intc:4.1)
                           irq irq -> IRQ_F2P[0] (GIC SPI 29)
```

The processor has only a few inputs for interrupts from the FPGA, and the overlay has many blocks that raise them. So
they all go to one **AXI interrupt controller**, `system_interrupts`, whose single output reaches the processor
(`IRQ_F2P[0]`). The buttons' GPIO is its input 13. That has consequences:

- only the controller has a device that Linux can wait on;
- the buttons' own constant, `pac::btns_gpio::IRQ_IP2INTC_IRPT`, has none: opening it returns
  `IrqError::ViaController`, which tells you to wait on the controller. The constant still says which input it is
  (`via_intc`), so your program need not hard-code 13.

## 2. Apply the interrupt overlay

Linux learns about the line from a small *device-tree overlay*, which the generator wrote:

```console
$ scp work/base/interrupts.dtso $BOARD: && ssh -t $BOARD sudo sh overlay.sh interrupts.dtso
```

The script says that it released the image's `40000000.fabric` device: the PYNQ image holds this line for itself until
then. It applies until the next boot, and one overlay per boot is the safe habit
([why](../../guide/hardware.md#troubleshooting)). `board_check` now passes all its steps.

## 3. The program

```rust,ignore
{{#include ../../../apps/base/linux/interrupts.rs}}
```

The sequence for any source behind a controller is:

1. `controller.init()` and `controller.enable(input)`: switch on the inputs you care about;
2. let the block raise its interrupt (`buttons.enable_interrupt(..)`);
3. `controller.wait(&irq, input, timeout)`: sleeps on the controller's line and returns the **pending bits** within
   `input`, or `None` on time-out;
4. clear the block (`buttons.clear_interrupt(..)`), **then** `controller.ack(input)`, in that order, or the input
   latches again at once.

`self_test` raises the buttons' interrupt from software, so the whole path is checked with nobody touching the board.

## 4. Run it

```console
$ cp apps/base/linux/interrupts.rs work/base/examples/
$ scripts/board/run.sh work/base interrupts
```

It prints the self test's result, then waits. Button 0 counts up on the LEDs, button 1 counts down, button 2 clears,
and button 3 ends the program.

## What you did

You handled an interrupt that reaches Linux through a controller: found the input, enabled it, slept on the
controller's line, then cleared the source and acknowledged. With several inputs enabled, the returned bits tell you
which one fired.

**Try next:** add the switches (their input is in `pac::switches_gpio`).

Continue with [Linux 3: Memory inside the FPGA](03-block-ram.md).
