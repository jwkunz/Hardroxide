# Zynq UltraScale+ on the RFSoC 4x2 (3.0 preview)

**Preview.** Zynq UltraScale+ support is being built for 3.0 ([ADR 0005](../../decisions/0005-ultrascale-plus.md)) and is
on the `v3` branch only. What works today is **bare metal and the real-time kernel**: processor start-up, the kernel, SD,
Ethernet and the board's own hardware. `hardroxide generate` does **not** accept UltraScale+ designs yet, so there are no
generated crates or FPGA drivers here, and there is no Linux support yet.

These pages run ready-made programs from `baremetal/` on a Real Digital RFSoC 4x2. The output shown was captured on one;
addresses and serial numbers are replaced.

| # | Tutorial | You learn |
|---|---|---|
| 1 | [First light](01-first-light.md) | set up the board, run a program over JTAG |
| 2 | [The base overlay](02-base-overlay.md) | the FPGA design: buttons, switches, LEDs and interrupts |
| 3 | [The board's own hardware](03-board-hardware.md) | identity, temperatures, power monitors, display, clock chips, the processor's LEDs and button |
| 4 | [The real-time kernel](04-kernel.md) | the kernel on a Cortex-A53, and its timing |
| 5 | [Network and SD card](05-network-sd.md) | a web and HTTPS server; FAT32, with care |

## One FPGA design: the base overlay

As in PYNQ, **every tutorial uses the same FPGA design**, the board's base overlay:
`fixtures/board/rfsoc4x2_base_overlay.xsa` ([what is in it](../../reference/base-overlay.md#rfsoc-4x2)). It is
RFSoC-PYNQ's own bitstream, with the processor start-up code that running without Linux needs. You do not build it and
need no Vivado license. Loading it over JTAG takes about half a minute.

## Host setup

- The AMD **Vitis** tools (2025.2 was used). Running programs needs no Vivado and no license.
- The board's JTAG udev rule and `hardroxide doctor`, as in [Running bare metal](../../guide/baremetal.md#host-setup-once).
- The Rust target: `rustup target add aarch64-unknown-none`.
- The command-line tool **from this checkout** (the released one is 2.x): `cargo install --path crates/hardroxide-cli`.

## Board setup

1. Set the board's boot mode switch to **JTAG**.
2. Connect the 12 V supply and the USB port labelled `PROG UART` (JTAG and the console, usually `/dev/ttyUSB1`).
3. For tutorial 5, an Ethernet cable to a network with a DHCP server (the tests use a direct cable to the host, which
   serves DHCP).
4. Switch on.

Nothing in tutorials 1 to 4 writes to storage; power-cycling the board undoes everything.
