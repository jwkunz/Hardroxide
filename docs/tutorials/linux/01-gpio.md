# Linux 1: LEDs, buttons and switches

**Design:** `fixtures/pynq_z2_gpio.xsa` · **Time:** 15 minutes · **Needs:** the
[setup](index.md#before-you-start-once) done once

This is the only design wired to real pins, so you can see and touch the result.
It contains the Zynq processing system (PL clock FCLK0 at 100 MHz) and one AXI
GPIO at `0x41200000`:

- **channel 1**: 4 outputs, wired to the board's four LEDs;
- **channel 2**: 6 inputs, wired to the four buttons (bits 3:0) and the two
  switches (bits 5:4).

You will inspect the design, generate a Rust crate from it, run the generated
checks on the board, then write and run a small program of your own. Everything
you do here is what you will do with your own design.

## 1. Look at the design

Before generating anything, ask Hardroxide what it sees:

```console
$ hardroxide inspect fixtures/pynq_z2_gpio.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: 0ec1a9e3...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x41200000      64 KiB  axi_gpio_0  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)

1 instances: 1 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
```

This is what you will get: one instance, `axi_gpio_0`, that becomes a field of
the generated `Design` with the curated `AxiGpio` driver. No interrupts, so no
interrupt overlay is needed. (`hardroxide inspect` also lists, for bigger designs,
instances it cannot give a driver to and instances the processor cannot reach.)

## 2. Generate the crate

```console
$ hardroxide generate fixtures/pynq_z2_gpio.xsa -o work/gpio --hardroxide-path .
generated 10 files in work/gpio
$ ls work/gpio work/gpio/src work/gpio/examples
work/gpio:
bitstream
Cargo.toml
examples
interface.sha256
interface.txt
src

work/gpio/examples:
board_check.rs
gpio_smoke.rs

work/gpio/src:
lib.rs
pac.rs
```

`--hardroxide-path .` makes the crate depend on the runtime crates in your
checkout. The pieces, in the order you will care about them:

- `bitstream/design.bin`: the FPGA configuration, embedded into your binary.
- `src/lib.rs`: the `Design` type. Open it and look at the struct: one typed field
  per instance.

  ```rust,ignore
  pub struct Design<B: MmioBackend> {
      pub axi_gpio_0: hardroxide_ip::gpio::AxiGpio<B::Region>,
      /* ... */
  }
  ```

- `src/pac.rs`: addresses, register offsets and bit-field masks as constants
  (`pac::axi_gpio_0::BASE_ADDRESS`, `pac::axi_gpio_0::gpio_data::OFFSET`, ...).
- `examples/`: checks you can run on the board right away. `board_check` and
  `gpio_smoke` exist because the design has a GPIO.

The generated crate is an ordinary Cargo crate; `cargo build` in it works on your
workstation too (it just cannot reach any hardware there).

## 3. First contact with the board

`board_check` loads the bitstream, reads each PL clock back from the chip to
compare with the design, and opens the UIO device of every interrupt. It touches
no IP register, so it cannot hang on a faulty IP:

```console
$ scripts/board/run.sh work/gpio board_check
1. loading bitstream and applying clocks ...
   ok
2. FCLK readback (SLCR) vs. the design:
   FCLK0: 100000000 Hz (want 100000000) ok
all checks passed; next run the per-IP examples
```

Step 1 is the moment your design is programmed into the FPGA. If it fails with
`PlInUse`, run `prep.sh` again (see the setup). Step 2 confirms the clock really
is 100 MHz. (There is no step 3 here because this design has no interrupts.)

## 4. Run the generated example

```console
$ scripts/board/run.sh work/gpio gpio_smoke
channel 2 inputs: 0x0
channel 2 inputs: 0x0
channel 2 inputs: 0x0
channel 2 inputs: 0x0
channel 2 inputs: 0x0
channel 2 inputs: 0x0
channel 2 inputs: 0x0
channel 2 inputs: 0x0
```

`gpio_smoke` walks a single lit LED across the four LEDs (watch the board) and
prints channel 2 each step. All zeros means no button is pressed and both
switches are down; with a switch up you will see `0x10` or `0x20`.

## 5. Write your own program

Here is a longer program: it walks the LEDs 2 seconds at a time and prints every
change of the buttons and switches. Read it; the driver calls are the whole
API you need for a GPIO.

```rust,ignore
{{#include ../../../fixtures/board_tests/gpio/gpio_interactive.rs}}
```

- `Design::load()` programs the FPGA, sets the clocks and maps the registers; it
  needs root, which `run.sh` provides.
- `d.axi_gpio_0` is the typed driver. `set_input_mask(Channel::One, 0)` makes
  every channel-1 pin an output, `write` sets the pins, `read` samples them.

Copy it into the crate's `examples/` and run it (it runs for 45 seconds; press
buttons and flip switches while it runs):

```console
$ cp fixtures/board_tests/gpio/gpio_interactive.rs work/gpio/examples/
$ scripts/board/run.sh work/gpio gpio_interactive
```

This is what appeared on a real board when the buttons were pressed one at a
time and the switches were toggled (only the input lines are shown):

```text
[ 0.0s] inputs 0x20  btn[3:0]=0000 sw[1:0]=10
[ 0.5s] inputs 0x00  btn[3:0]=0000 sw[1:0]=00
[ 1.1s] inputs 0x10  btn[3:0]=0000 sw[1:0]=01
[ 1.9s] inputs 0x11  btn[3:0]=0001 sw[1:0]=01
[ 2.1s] inputs 0x10  btn[3:0]=0000 sw[1:0]=01
[ 2.4s] inputs 0x12  btn[3:0]=0010 sw[1:0]=01
[ 2.9s] inputs 0x14  btn[3:0]=0100 sw[1:0]=01
[ 4.5s] inputs 0x18  btn[3:0]=1000 sw[1:0]=01
[ 6.0s] inputs 0x30  btn[3:0]=0000 sw[1:0]=11
```

Each button is one bit, so you can read `pac` straight off the board: bit 0 is
the first button, bit 4 is the first switch.

## 6. Test your logic without a board

The same crate works against a fake memory, so your program's logic can be
unit-tested on your workstation. `MockBackend` stands in for `/dev/mem`:

```rust,ignore,fixture=pynq_z2_gpio
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::MockBackend;

let be = MockBackend::new();
be.poke(0x4120_0008, 0b10_0101);              // pretend buttons 0 and 2 and a switch are on
let d = my_design::Design::map_only(be.clone())?;

d.axi_gpio_0.write(Channel::One, 0b1010)?;    // light two LEDs
assert_eq!(be.peek(0x4120_0000), 0b1010);     // they reached the data register
assert_eq!(d.axi_gpio_0.read(Channel::Two)?, 0b10_0101);
```

The mock does not simulate hardware behaviour (it just stores values), so it
tests *your* code, not the FPGA. See
[Using a generated crate](../../guide/using.md) for what it can and cannot do.

## What you did

You turned an XSA into a crate (`inspect`, `generate`), checked it on the board
(`board_check`), ran generated and hand-written code against it, and tested
against a mock. The next tutorials only change *which design* and *which IP*.

**Try next:** make the LEDs follow the buttons; or read `pac.rs` and find the
direction register of channel 2.

Continue with [Linux 2: Timer, SPI and interrupts](02-timer-spi-irq.md).
