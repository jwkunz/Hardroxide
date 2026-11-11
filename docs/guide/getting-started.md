# Getting started

## Requirements

- Rust (latest stable, edition 2024).
- An `.xsa` exported from Vivado (the tool was developed against 2025.2).
  Hardroxide itself does **not** need Vivado installed.
- To run on a board: a PYNQ-Z2 with the stock PYNQ image and root access.

## Build the tool

```console
$ git clone https://github.com/jwkunz/Hardroxide
$ cd Hardroxide
$ cargo build --release -p hardroxide-cli
$ ./target/release/hardroxide --help
```

## Inspect a design

```console
$ hardroxide inspect fixtures/pynq_z2_gpio.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: 0ec1a9e3...
FCLK0: 100000000 Hz
0x41200000-0x4120ffff  axi_gpio_0  (xilinx.com:ip:axi_gpio:2.0)
```

## Generate and build a crate

```console
$ hardroxide generate fixtures/pynq_z2_gpio.xsa -o my_design \
      --hardroxide-path .
$ cd my_design && cargo build
```

`--hardroxide-path` makes the crate depend on the runtime crates by path
(use it while they are unpublished; the path must be valid wherever you build
the crate).

## Build for the board

The PYNQ-Z2's ARM cores run 32-bit Linux, so binaries for it use the
`armv7-unknown-linux-gnueabihf` target. Either build **on the board** (install
Rust there with `rustup`; slow but no setup), or **cross-compile** on your
workstation:

```console
$ rustup target add armv7-unknown-linux-gnueabihf
$ sudo apt install gcc-arm-linux-gnueabihf      # Debian/Ubuntu cross linker
```

Tell Cargo which linker to use, in `my_design/.cargo/config.toml`:

```toml
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
```

Then build, copy and run (the board's default user on the PYNQ image is
`xilinx`; root is needed for `/dev/mem` and the FPGA manager):

```console
$ cargo build --release --target armv7-unknown-linux-gnueabihf --example board_check
$ scp target/armv7-unknown-linux-gnueabihf/release/examples/board_check xilinx@pynq:
$ ssh xilinx@pynq sudo ./board_check
```

A cross-compiled binary links against the workstation's glibc. If the board
reports `GLIBC_2.xx not found`, its glibc is older than your toolchain's:
build on the board, or build against an older glibc (for example
`cargo zigbuild --target armv7-unknown-linux-gnueabihf.2.35`, matching the
board's `ldd --version`). Cross-building and running on the board is not
yet verified ([G8](../08-verification-gaps.md)).

You can run the runtime crates' own tests as armv7 binaries without a board,
under `qemu-arm` user-mode emulation: `scripts/test-armv7.sh` (needs
`gcc-arm-linux-gnueabihf` and `qemu-user`, or the copies bundled with Vitis).

Next: [How it works](concepts.md), or straight to
[Generating a crate](generating.md).
