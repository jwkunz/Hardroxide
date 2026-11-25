# RFSoC 2: The base overlay

`baremetal/base-overlay` drives the [base overlay](../../reference/base-overlay.md#rfsoc-4x2)'s buttons, switches and
LEDs with the same drivers the Zynq-7000 uses ([`AxiGpio`](../../reference/ip-gpio.md),
[`AxiIntc`](../../reference/ip-intc.md)), and takes their interrupts.

```console
$ cd baremetal/base-overlay
$ cargo build --release
$ hardroxide run target/aarch64-unknown-none/release/base-overlay \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 70
```

```text
== RFSoC 4x2 base overlay ==
FPGA temperature: 41 C
LEDs follow what is written: true
buttons interrupt: quiet before true, arrived true, cleared true
switches interrupt: quiet before true, arrived true, cleared true
SELF TEST OK

now press the buttons and move the switches (60 s)
```

It first tests itself: it writes patterns to the white and colour LEDs, then raises each GPIO's interrupt from software
and follows it through the AXI interrupt controller to the processor. Then, for a minute, it is yours:

| Do | Get |
|---|---|
| button 0 | the count on the four white LEDs goes up |
| button 1 | it goes down |
| button 2 | it clears |
| switches 0, 1, 2, then button 3 | both colour LEDs show the colour: red, green, blue and their mixtures |

## How it is put together

```text
button or switch --> its AXI GPIO --> AXI interrupt controller --> pl_ps_irq0[0] --> processor (interrupt 121)
```

- The program names each block by its address in the overlay. A generated crate will do that for you when the generator
  supports this family; until then the addresses are at the top of `src/main.rs`.
- The order of an interrupt's handling matters: clear the source (the GPIO) first, then the controller.
- The overlay wires each colour LED as blue, green, red; `colour_bits` turns that around.

Next: [the board's own hardware](03-board-hardware.md).
