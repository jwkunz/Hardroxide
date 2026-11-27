# RFSoC 8: The base overlay under Linux

**Board:** [RFSoC 4x2](../../boards/rfsoc-4x2.md) · **Runs:** Linux

The same FPGA design and the same drivers as in [tutorial 2](02-base-overlay.md), now in an ordinary program under the
board's PYNQ Linux. Nothing here needs JTAG or the Vitis tools to run; what stays the same is the generated crate.

## 1. Board and host

- A card with RFSoC-PYNQ's image (4.0.0 was used), the boot mode switch on **SD**, an Ethernet cable and power. Log in
  with the image's default user, `xilinx`. On a cable straight to your computer the image answers at its fixed address,
  `192.168.2.99` (give your computer another one in `192.168.2.x`).
- **Wait about two minutes after switching on** before loading a design. The image's start-up script (`/boot/boot.py`)
  programs the board's clock chips and loads its own copy of the base overlay then, which would replace yours under a
  running program.
- On your computer: `rustup target add aarch64-unknown-linux-gnu`, a cross linker (`gcc-aarch64-linux-gnu` on Debian and
  Ubuntu; the helper below also finds the one in an AMD Vitis installation) and the command-line tool from this checkout.

## 2. Generate the crate

From the repository's root; Linux is the generator's default platform:

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --name design_1 -o ../base-linux --hardroxide-path .
$ cp apps/rfsoc4x2/linux/*.rs ../base-linux/examples/
$ cp -r apps/rfsoc4x2/common ../base-linux/examples/
```

## 3. First contact

`scripts/board/run.sh` builds an example, copies it to your home directory on the board and runs it there with `sudo`
(which asks for the password):

```console
$ export BOARD=xilinx@192.168.2.99
$ scripts/board/run.sh ../base-linux board_check
```

```text
1. loading bitstream and applying clocks ...
   ok
2. PL clock readback vs. the design:
   pl_clk0: 99999990 Hz (want 99999985) ok
   pl_clk1: 299999970 Hz (want 299999939) ok
3. interrupt UIO devices (needs interrupts.dtso applied):
   axi_intc_0.irq: ok
all checks passed; next run the per-IP examples
```

The program carries the bitstream inside it and gives it to Linux to load. This **replaces whatever design is in the
FPGA**: shut down any notebook that is using an overlay first. There is no device-tree overlay to apply for this design;
its one interrupt line is one the image already knows.

## 4. Lights and buttons

```console
$ scripts/board/run.sh ../base-linux lights 5
```

```text
== RFSoC 4x2 base overlay under Linux ==
FPGA temperature: 52 C
LEDs follow what is written: true
buttons interrupt: quiet before true, arrived true, cleared true
switches interrupt: quiet before true, arrived true, cleared true
SELF TEST OK

now press the buttons and move the switches (5 s)
done
```

The number is how many seconds it then listens to you (60 without it). The controls are those of
[tutorial 2](02-base-overlay.md): button 0 counts up on the white LEDs, 1 down, 2 clears, and 3 lights the colour LEDs
in the colour switches 0 to 2 choose.

## What changes from bare metal

| | Bare metal (tutorial 2) | Linux |
|---|---|---|
| Start-up | `Design::init()`: the FPGA was loaded over JTAG | `Design::load()`: the program loads it |
| Registers | the program owns the processor | through `/dev/mem`, which needs root |
| Interrupts | `IrqWait::for_line(&line)` | `line.open()`, a UIO device; the program sleeps in the kernel until it fires |
| Errors and printing | `println!` over the serial port | an ordinary program: `?`, command-line arguments, the network |
| Timing | microseconds, repeatable | whatever Linux gives you |

The drivers and the calls on them (`d.leds_gpio.write(...)`, `controller.wait(...)`) are identical.

If a program will not start on the board with ``version `GLIBC_...' not found``, your linker's C library is newer than
the image's: build with the Vitis linker (set `LINKER`). More about the image, DMA memory and the limits:
[Linux on Zynq UltraScale+](../../guide/linux-ultrascale.md).

Next: [a web page for the board](09-linux-web.md).
