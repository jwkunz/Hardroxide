# Getting started

Two ways in: **try it** with the XSAs in this repository (no Vivado, no board for the
first half), or **use your own design** ([the workflow](workflow.md)). Either way,
this page gets the tool and the toolchain ready. When you are, the
[tutorials](../tutorials/index.md) are the fastest way to learn it by doing.

## Requirements

- Rust (latest stable, edition 2024).
- An `.xsa` exported from Vivado. Hardroxide itself does **not** need Vivado
  installed; the repository has ready-made ones in `fixtures/`. To make your own,
  see [Exporting an XSA from Vivado](vivado.md).
- To run on hardware, depending on the [flavor](workflow.md#choosing-a-flavor):
  - **Linux:** a PYNQ-Z2 on the network (the stock PYNQ image, root access), and a cross linker for 32-bit ARM Linux.
  - **Bare metal / RTOS:** the AMD Vitis tools and a USB (JTAG) cable; no cross linker. See
    [Running bare metal](baremetal.md).

## Build the tool

```console
$ git clone https://github.com/jwkunz/Hardroxide
$ cd Hardroxide
$ cargo build --release -p hardroxide-cli
$ export PATH=$PWD/target/release:$PATH
$ hardroxide --help
```

Or install the released binary from crates.io (version 2.7.0 and later; it needs no checkout):

```console
$ cargo install hardroxide-cli
```

## Inspect a design

```console
$ hardroxide inspect fixtures/pynq_z2_gpio.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: 0ec1a9e3...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x41200000      64 KiB  axi_gpio_0  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)

1 instances: 1 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
```

This tells you what you will get before generating anything: which instances
become typed drivers, which are generated HLS wrappers, which are raw register
windows (no driver), where each interrupt goes, and which instances are not
reachable from the PS at all.

## Generate and build a crate

```console
$ hardroxide generate fixtures/pynq_z2_gpio.xsa -o my_design \
      --hardroxide-path .
$ cd my_design && cargo build
```

By default the generated crate depends on the runtime crates published on crates.io (version `"2"`); `--hardroxide-path` makes it depend on the runtime crates in your checkout
instead (to develop the runtime or to try a change that is not released; the path is made absolute for you). The crate
builds on your workstation, where it can be tested against a mock backend
([Using a generated crate](using.md)); it can only reach hardware on the board.

## Build for the board (Linux flavor)

The PYNQ-Z2's ARM cores run 32-bit Linux, so binaries for it use the
`armv7-unknown-linux-gnueabihf` target. Either build **on the board** (install Rust
there with `rustup`; slow but no setup), or **cross-compile** on your workstation:

```console
$ rustup target add armv7-unknown-linux-gnueabihf
$ sudo apt install gcc-arm-linux-gnueabihf      # Debian/Ubuntu cross linker
```

The simplest way to build, copy and run is the helper script, which finds the
linker for you (see [Board scripts](../reference/scripts.md)):

```console
$ export BOARD=xilinx@pynq
$ scripts/board/run.sh my_design board_check
```

Doing it by hand: tell Cargo which linker to use in `my_design/.cargo/config.toml`,

```toml
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
```

then build, copy and run (the board's default user on the PYNQ image is `xilinx`;
root is needed for `/dev/mem` and the FPGA manager):

```console
$ cd my_design
$ cargo build --release --target armv7-unknown-linux-gnueabihf --example board_check
$ scp target/armv7-unknown-linux-gnueabihf/release/examples/board_check xilinx@pynq:
$ ssh -t xilinx@pynq sudo ./board_check
```

Before the first run, free the PL of whatever the stock image keeps using it for
([Running on hardware](hardware.md#one-time-board-setup)); `load()` refuses
otherwise.

**C library versions.** A cross-compiled binary links against your toolchain's glibc.
Binaries built with the Vitis-bundled gcc used only glibc symbols up to 2.34 and ran
on the board's glibc 2.35. If the board reports `GLIBC_2.xx not found`, its glibc is
older than your toolchain's: build on the board, or build against an older glibc
(for example `cargo zigbuild --target armv7-unknown-linux-gnueabihf.2.35`, matching
the board's `ldd --version`).

You can run the runtime crates' own tests as armv7 binaries without a board, under
`qemu-arm` user-mode emulation: `scripts/test-armv7.sh` (needs
`gcc-arm-linux-gnueabihf` and `qemu-user`, or the copies bundled with Vitis).

## Next

- **Learn by doing:** [Linux 1: LEDs, buttons and switches](../tutorials/linux/01-gpio.md), or for the other
  flavors [Bare metal 1](../tutorials/baremetal/01-jtag.md) and [RTOS 1](../tutorials/rtos/01-control-loop.md).
- **The whole picture:** [The workflow](workflow.md), then
  [How it works](concepts.md).
- **Your own design:** [Exporting an XSA from Vivado](vivado.md).
