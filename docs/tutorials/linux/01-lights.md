# Linux 1: LEDs, buttons and switches

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 15 minutes · **Needs:** the
[setup](index.md#before-you-start-once) done once

The PYNQ base overlay is the FPGA design that comes with your board. You will inspect it, generate a Rust crate from it,
check it on the board, and write to the LEDs. Everything you do here is what you will do with any design.

> **Status:** `board_check` below was captured on a PYNQ-Z2. This tutorial's own program is new: it builds against the
> generated crate and has **not yet been run on a board**.

## 1. Look at the design

```console
$ hardroxide inspect fixtures/board/pynq_base_overlay.xsa
```

It lists every block the processor can reach. The four this tutorial uses:

```text
0x41200000      64 KiB  switches_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
0x41210000      64 KiB  btns_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
0x41240000      64 KiB  rgbleds_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
0x41250000      64 KiB  leds_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
```

An **AXI GPIO** is a block of input and output pins that the processor reads and writes. Here the design has one for the
two switches, one for the four buttons, one for the two colour LEDs (three pins each) and one for the four LEDs.
(`inspect` also shows which interrupt each has; that is [tutorial 2](02-interrupts.md).)

## 2. Generate the crate

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa -o work/base --hardroxide-path . --name design_1
$ ls work/base/examples
board_check.rs  bram_test.rs  dma_loopback.rs  gpio_smoke.rs  hls_smoke.rs  irq_probe.rs
```

`--name design_1` names the crate, and the tutorials' programs use that name. `--hardroxide-path .` makes the crate
depend on the runtime crates in your checkout. The pieces, in the order you will care about them:

- `bitstream/design.bin`: the FPGA configuration, embedded into your program.
- `src/lib.rs`: the `Design` type. Open it and look at the struct: one typed field per block.

  ```rust,ignore
  pub struct Design<B: MmioBackend> {
      pub leds_gpio: hardroxide_ip::gpio::AxiGpio<B::Region>,
      /* ... */
  }
  ```

- `src/pac.rs`: addresses, register offsets and bit-field masks as constants
  (`pac::leds_gpio::BASE_ADDRESS`, ...).
- `examples/`: checks you can run on the board right away.

The generated crate is an ordinary Cargo crate; `cargo build` in it works on your workstation too (it just cannot reach
any hardware there).

## 3. First contact with the board

`board_check` loads the bitstream, reads each PL clock back from the chip to compare with the design, and opens the
device of every interrupt. It touches no block's registers, so it cannot hang on a faulty one:

```console
$ scripts/board/run.sh work/base board_check
1. loading bitstream and applying clocks ...
   ok
2. FCLK readback (SLCR) vs. the design:
   FCLK0: 100000000 Hz (want 100000000) ok
   FCLK1: 142857142 Hz (want 142857132) ok
   FCLK2: 200000000 Hz (want 200000000) ok
   FCLK3: 100000000 Hz (want 100000000) ok
3. interrupt UIO devices (needs interrupts.dtso applied):
   system_interrupts.irq: no UIO device for interrupt `system_interrupts.irq` (device-tree node `system_interrupts_irq`, base 0x41800000); is the generated overlay (interrupts.dtso) applied?
1 check(s) failed
```

Step 1 is the moment the design is programmed into the FPGA. If it fails with `PlInUse`, run `prep.sh` again (see the
setup). Step 2 confirms the four clocks. **Step 3 is expected to fail here:** interrupts need one more step, which is
tutorial 2.

## 4. Your own program

```rust,ignore
{{#include ../../../apps/base/linux/lights.rs}}
```

- `Design::load()` programs the FPGA, sets the clocks and maps the registers; it needs root, which `run.sh` provides.
- `d.leds_gpio` is the typed driver. `set_input_mask(Channel::One, 0)` makes every pin an output, `write` sets the
  pins, `read` samples them.

```console
$ cp apps/base/linux/lights.rs work/base/examples/
$ scripts/board/run.sh work/base lights
```

The four LEDs light one after the other. Then press the buttons and move the switches for half a minute: each LED
follows its button, the switches choose the colour of the two colour LEDs (off, red, green, blue), and each change is
printed.

## 5. Test your logic without a board

The same crate works against a fake memory, so your program's logic can be unit-tested on your workstation.
`MockBackend` stands in for `/dev/mem`:

```rust,ignore,fixture=board/pynq_base_overlay
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::MockBackend;

let be = MockBackend::new();
be.poke(0x4121_0000, 0b0101);                    // pretend buttons 0 and 2 are held down
let d = my_design::Design::map_only(be.clone())?;

let pressed = d.btns_gpio.read(Channel::One)?;
d.leds_gpio.write(Channel::One, pressed)?;       // what your program would do
assert_eq!(be.peek(0x4125_0000), 0b0101);        // it reached the LEDs' data register
```

The mock does not simulate hardware (it just stores values), so it tests *your* code, not the FPGA. See
[Using a generated crate](../../guide/using.md) for what it can and cannot do.

## What you did

You turned an XSA into a crate (`inspect`, `generate`), checked it on the board (`board_check`), ran your own program
against it, and tested against a mock.

**Try next:** make the buttons count in binary on the LEDs; or read `pac.rs` and find the direction register of a GPIO.

Continue with [Linux 2: Interrupts](02-interrupts.md).
