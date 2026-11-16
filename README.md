<p align="center">
  <img src="docs/assets/logo.png" alt="Hardroxide logo" width="360">
</p>

# Hardroxide

Rust HAL generator and runtime for AMD FPGAs: turn a Vivado XSA into a typed
Rust crate, a Rust counterpart to the PYNQ overlay framework.

```
hardroxide generate design.xsa -o my_design
```

Documentation (user guide, API reference, design docs):
<https://jwkunz.github.io/Hardroxide/>, or build it locally with `mdbook build`
(output in `target/book`).

## What it does

- Reads a Vivado 2025.2 XSA (bitstream + hardware handoff) for a **Zynq-7000**
  design and generates a Cargo crate: the bitstream embedded and ready to load,
  one typed field per IP instance the processor can reach, interrupt wiring as a
  device-tree overlay, and smoke-test examples.
- Curated drivers for AXI GPIO, DMA, Timer, IIC, Quad SPI, UART Lite, BRAM
  controller, interrupt controller and AXI4-Stream FIFO (with `embedded-hal` 1.0
  where it fits), generated wrappers for Vitis HLS kernels (including pointer
  arguments), and raw register windows for everything else.
- `hardroxide inspect` shows what a design will become before you generate;
  `hardroxide update` swaps in a rebuilt bitstream without touching the Rust
  source when the interface did not change.

## Status

Version 1.0. Validated on a **PYNQ-Z2** (PYNQ 3.1, kernel 6.6): bitstream
loading, PL clocks, GPIO, HLS (scalar and pointer arguments), DMA, Timer, IIC,
SPI (internal loopback), UART, BRAM, FIFO, interrupts (including through an AXI
interrupt controller), and a crate generated from the PYNQ base overlay. The
results, and what is still unchecked, are in
[`docs/08-verification-gaps.md`](docs/08-verification-gaps.md).

Limits: Zynq-7000 only (UltraScale+ designs are refused with a clear error),
Linux userspace on the PS, direct-register-mode DMA only, one register block per
IP instance, and Vivado 2025.2 is the version actually exercised on hardware.

## Quick start

```
cargo install --path crates/hardroxide-cli      # installs `hardroxide`
hardroxide inspect design.xsa
hardroxide generate design.xsa -o my_design
```

then follow the [user guide](https://jwkunz.github.io/Hardroxide/guide/getting-started.html)
to cross-build for the board (`armv7-unknown-linux-gnueabihf`) and run
`board_check` first.

Before reprogramming the PL, stop anything on the board that uses it (a base
overlay's HDMI or GPIO drivers, for example): reprogramming under a live driver
can freeze the CPU. `load()` refuses when it detects one. See
[Running on hardware](https://jwkunz.github.io/Hardroxide/guide/hardware.html).

## License

Copyright (c) 2026 Numerius Engineering LLC. Licensed under either of
[Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at
your option. Contact: numerius.engineering@gmail.com.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
