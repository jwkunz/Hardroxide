# Bare metal 2: A PL interrupt through the GIC

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 15 minutes · **Needs:** [Bare metal 1](01-jtag.md)

Under [Linux](../linux/02-interrupts.md) an interrupt from the programmable logic arrives as a read on a UIO device.
With no operating system there is no UIO: the interrupt goes from the PL to the processor's interrupt controller (the
**GIC**), and `hardroxide-ps7` programs the GIC for you. The generated crate knows from the design which GIC line each
interrupt uses and whether it is level or edge triggered.

## 1. The route

`board_check` printed it in tutorial 1: `irq system_interrupts.irq -> GIC id 61 (level)`. All of the overlay's
interrupts go to one **AXI interrupt controller**, `system_interrupts`, whose single output is that line; the buttons'
GPIO is its input 13 (`pac::btns_gpio::IRQ_IP2INTC_IRPT.via_intc`).

## 2. The program

```rust,ignore
{{#include ../../../apps/base/baremetal/button_irq.rs}}
```

- `IrqWait` is the bare-metal implementation of the `IrqWaiter` trait that every driver accepts (the Linux one reads a
  UIO file). It sleeps the core until the handler has seen the interrupt; your code writes no handler.
- `d.enable_interrupts()` opens the GIC and the processor's interrupt mask; nothing arrives before it.
- For a source behind the controller: clear the block, **then** acknowledge the controller.

## 3. Run it

```console
$ cp ../../apps/base/baremetal/button_irq.rs examples/
$ cargo build --release --example button_irq
$ hardroxide run target/armv7a-none-eabihf/release/examples/button_irq --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 25
```

It first raises the buttons' interrupt from software, so the path is checked with nobody at the board, then waits for
real presses. On a PYNQ-Z2:

```text
== button_irq: the buttons' interrupt is input 13 of the controller ==
buttons interrupt: quiet before true, arrived true, cleared true
SELF TEST OK

now press the buttons (20 s)
buttons 0001 pressed: count 1
buttons 0010 pressed: count 0
buttons 0100 pressed: count 0
buttons 0001 pressed: count 1
buttons 0001 pressed: count 2
buttons 0001 pressed: count 3
buttons 0001 pressed: count 4
buttons 0001 pressed: count 5
buttons 0010 pressed: count 4
buttons 0010 pressed: count 3
buttons 0010 pressed: count 2
buttons 0100 pressed: count 0
buttons 0100 pressed: count 0
buttons 0010 pressed: count 15
done
```

Counting down from 0 gives 15: the count has the four LEDs' four bits.

For a periodic interrupt from a timer, measured to the microsecond, see
[Your own design 8](../own-design/08-baremetal-timer.md). For several things happening at once, each with its own
deadline, go on to the [RTOS tutorials](../rtos/index.md).
