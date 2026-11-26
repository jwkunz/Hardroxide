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

## Let the generator name the blocks

The addresses above can come from the generator instead. From the repository's root:

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --platform baremetal \
    -o ../base --hardroxide-path .
$ cd ../base
$ cargo build --release --examples
$ hardroxide run target/aarch64-unknown-none/release/examples/board_check \
    --xsa ../Hardroxide/fixtures/board/rfsoc4x2_base_overlay.xsa
```

```text
== board check (base) ==
PL configured, interface 104a033a6d9c944f8c517250bb83303ec94c8a897c9c75f00b61e36e9421c9de
FCLK0: 99999990 Hz (want 99999985) ok
FCLK1: 299999970 Hz (want 299999939) ok
irq axi_intc_0.irq -> GIC id 121 (level)
all checks passed; next run the per-IP examples
```

The crate has a `Design` with a field for every block the processor reaches, each with its driver, so the program
starts like this and names no address:

```rust,ignore
let d = base::Design::init()?;                       // check the FPGA, set up the processor, map every block
d.leds_gpio.set_input_mask(Channel::One, 0)?;
d.leds_gpio.write(Channel::One, 0b0101)?;
let irq = IrqWait::for_line(&base::pac::axi_intc_0::IRQ_IRQ);   // the interrupt, as the design wires it
```

`gpio_smoke` walks the white LEDs, `bram_test` checks a Pmod processor's memory, and
`fixtures/board_tests_rfsoc/base/tour.rs` takes the buttons' and switches' interrupts this way (copy it into
`examples/`, generating with `--name design_1`). With `--platform rtos` the crate brings the kernel and an example of
it. The generator's support for this family is new ([what has been checked](../../reference/baremetal.md#zynq-ultrascale)).

## How it is put together

```text
button or switch --> its AXI GPIO --> AXI interrupt controller --> pl_ps_irq0[0] --> processor (interrupt 121)
```

- This program names each block by its address in the overlay, at the top of `src/main.rs`, so that you can see them;
  the generated crate does that for you.
- The order of an interrupt's handling matters: clear the source (the GPIO) first, then the controller.
- The overlay wires each colour LED as blue, green, red; `colour_bits` turns that around.

Next: [the board's own hardware](03-board-hardware.md).
