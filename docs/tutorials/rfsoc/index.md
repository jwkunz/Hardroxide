# Zynq UltraScale+ on the RFSoC 4x2

New in 3.0 ([plan and results](../../decisions/0005-ultrascale-plus.md)): **bare metal and the real-time kernel**
(processor start-up, the kernel, SD, Ethernet, the board's own hardware) and **Linux** under the board's PYNQ image,
each with crates generated from your design and the FPGA drivers, the radio's among them. The RFSoC 4x2 is the one
UltraScale+ board it has been run on.

These pages run ready-made programs on a Real Digital RFSoC 4x2. Those that use the FPGA are applications in
`apps/rfsoc4x2/`, built against a generated crate; those that test the processor system and the board's own hardware are
in `baremetal/`. The output shown was captured on one; addresses and serial numbers are replaced.

| # | Tutorial | You learn |
|---|---|---|
| 1 | [First light](01-first-light.md) | set up the board, run a program over JTAG |
| 2 | [The base overlay](02-base-overlay.md) | the FPGA design: buttons, switches, LEDs and interrupts |
| 3 | [The board's own hardware](03-board-hardware.md) | identity, temperatures, power monitors, display, clock chips, the processor's LEDs and button |
| 4 | [A mood light on the kernel](04-mood-light.md) | tasks, priorities, interrupts that wake tasks, periodic work |
| 5 | [The board's hardware as tasks](05-kernel-board.md) | slow and urgent work together, sharing data safely |
| 6 | [The kernel's tests and timing](06-kernel-timing.md) | what the kernel guarantees, measured |
| 7 | [Network and SD card](07-network-sd.md) | a web and HTTPS server, also as a task beside a control loop; FAT32, with care |
| 8 | [The base overlay under Linux](08-linux.md) | the same design from a Linux program: loading, interrupts |
| 9 | [A web page for the board](09-linux-web.md) | the network and the kernel's drivers alongside the FPGA |
| 10 | [The FPGA's own memory](10-linux-memory.md) | the second 4 GiB of DDR4, and what must come first |
| 11 | [A tone out and back](11-radio.md) | the radio: tones from the DACs, recorded and found in the spectrum, on all three platforms |

## One FPGA design: the base overlay

As in PYNQ, **every tutorial uses the same FPGA design**, the board's base overlay:
`fixtures/board/rfsoc4x2_base_overlay.xsa` ([what is in it](../../reference/base-overlay.md#rfsoc-4x2)). It is
RFSoC-PYNQ's own bitstream, with the processor start-up code that running without Linux needs. You do not build it and
need no Vivado license. Loading it over JTAG takes about half a minute.

## Host setup

- The AMD **Vitis** tools (2025.2 was used). Running programs needs no Vivado and no license.
- The board's JTAG udev rule and `hardroxide doctor`, as in [Running bare metal](../../guide/baremetal.md#host-setup-once).
- The Rust target: `rustup target add aarch64-unknown-none`.
- A checkout of this repository (the pages use its designs and programs) and the command-line tool, version 3:
  `cargo install --path crates/hardroxide-cli` from the checkout, or `cargo install hardroxide-cli`.

## Board setup

For tutorials 8 to 10 (Linux), see [tutorial 8](08-linux.md). For the others:

1. Set the board's boot mode switch to **JTAG**.
2. Connect the 12 V supply and the USB port labelled `PROG UART` (JTAG and the console, usually `/dev/ttyUSB1`).
3. For tutorial 7, an Ethernet cable to a network with a DHCP server (the tests use a direct cable to the host, which
   serves DHCP).
4. Switch on.

Nothing in tutorials 1 to 6 writes to storage; power-cycling the board undoes everything.
