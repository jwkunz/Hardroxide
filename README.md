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
https://jwkunz.github.io/Hardroxide/ (once GitHub Pages is enabled), or build
it locally with `mdbook build` (output in `target/book`).

Status: hardware-free development is complete through Milestone 5 (GPIO, HLS,
DMA, Timer, IIC, SPI); nothing has been validated on a board yet. See
[`docs/`](docs/README.md), especially
[`docs/08-verification-gaps.md`](docs/08-verification-gaps.md).

License: MIT OR Apache-2.0.
