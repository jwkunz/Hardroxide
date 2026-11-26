# RFSoC 2: The base overlay

The [base overlay](../../reference/base-overlay.md#rfsoc-4x2) is the FPGA design all these tutorials use. You do not
write its addresses down: the generator reads the design and writes a crate with a field for every block, each with its
driver.

## 1. Generate the crate

From the repository's root:

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --platform baremetal \
    --name design_1 -o ../base --hardroxide-path .
$ cd ../base
$ cargo build --release --example board_check
$ hardroxide run target/aarch64-unknown-none/release/examples/board_check \
    --xsa ../Hardroxide/fixtures/board/rfsoc4x2_base_overlay.xsa
```

```text
== board check (design_1) ==
PL configured, interface 9c52b6a36c78b78b77edb2e1a076b967de0f01207bf5d6eafddfa49068bcdf03
FCLK0: 99999990 Hz (want 99999985) ok
FCLK1: 299999970 Hz (want 299999939) ok
irq axi_intc_0.irq -> GIC id 121 (level)
all checks passed; next run the per-IP examples
```

## 2. Lights and buttons

Copy `apps/rfsoc4x2/baremetal/lights.rs` into the crate's `examples/`, then:

```console
$ cargo build --release --example lights
$ hardroxide run target/aarch64-unknown-none/release/examples/lights \
    --xsa ../Hardroxide/fixtures/board/rfsoc4x2_base_overlay.xsa --watch 70
```

```text
== RFSoC 4x2 base overlay ==
FPGA temperature: 45 C
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

```rust,ignore
let d = Design::init()?;                          // check the FPGA, set up the processor, map every block
d.leds_gpio.set_input_mask(Channel::One, 0)?;     // a block is a field, with its driver
d.leds_gpio.write(Channel::One, 0b0101)?;
let irq = IrqWait::for_line(&pac::axi_intc_0::IRQ_IRQ);       // the interrupt, as the design wires it
let (_, input) = pac::btns_gpio::IRQ_IP2INTC_IRPT.via_intc?;  // which controller input the buttons feed
```

- The order of an interrupt's handling matters: clear the source (the GPIO) first, then the controller.
- The overlay wires each colour LED as blue, green, red; `colour_bits` turns that around.
- Where the driver has no method for something, the register window is there: `gpio.regs()`.

Next: [the board's own hardware](03-board-hardware.md).
