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
(use it while they are unpublished). Next: [Generating a crate](generating.md).
