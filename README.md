<p align="center">
  <img src="docs/assets/logo.png" alt="Hardroxide logo" width="360">
</p>

# Hardroxide

Rust HAL generator and runtime for AMD FPGAs: turn a Vivado XSA into a typed
Rust crate, a Rust counterpart to the PYNQ overlay framework.

```
hardroxide generate design.xsa -o my_design
```

Documentation: <https://jwkunz.github.io/Hardroxide/> (book at `/book/`, API reference at `/api/`), or build it locally with
`scripts/build-site.sh` (output in `target/site`).

- **New here?** Start with the [tutorials](https://jwkunz.github.io/Hardroxide/book/tutorials/index.html),
  grouped by flavor (Linux, bare metal, RTOS): twenty-one step-by-step walk-throughs with real
  output from a PYNQ-Z2. No Vivado needed.
- **Your own design?** [The workflow](https://jwkunz.github.io/Hardroxide/book/guide/workflow.html)
  and [Exporting an XSA from Vivado](https://jwkunz.github.io/Hardroxide/book/guide/vivado.html).
- **Looking something up?** The [reference](https://jwkunz.github.io/Hardroxide/book/reference/cli.html)
  and the [verification status](docs/08-verification-gaps.md).

## What it does

- Reads a Vivado 2025.2 XSA (bitstream + hardware handoff) for a **Zynq-7000**
  design and generates a Cargo crate: the bitstream embedded and ready to load,
  one typed field per IP instance the processor can reach, interrupt wiring as a
  device-tree overlay, and smoke-test examples.
- Three flavors, chosen with `--platform`: **linux** (a userspace program on the PYNQ image),
  **baremetal** (no operating system: startup, interrupts, timers, UART, MMU and caches from
  `hardroxide-ps7`) and **rtos** (bare metal plus the `hardroxide-rt` kernel with prioritised tasks,
  semaphores, mutexes and queues). `hardroxide run` and `hardroxide package` start bare-metal programs
  over JTAG or from SD with the Vitis tools.
- Curated drivers for AXI GPIO, DMA, Timer, IIC, Quad SPI, UART Lite, BRAM
  controller, interrupt controller and AXI4-Stream FIFO (with `embedded-hal` 1.0
  where it fits), generated wrappers for Vitis HLS kernels (including pointer
  arguments), and raw register windows for everything else.
- Applications on the PYNQ-Z2's base overlay, all with their own libraries and no third-party code: **HDMI graphics**, **audio** (WAV, tones, recording, an interrupt-driven pump), an **SD card with FAT32**,
  **Ethernet with a TCP/IP stack and web server**, **HTTPS with TLS 1.3** (a home-made, unaudited implementation) and an **integrated RTOS application** that serves a dashboard over TLS while a 10 ms control loop
  keeps its deadline. Seven tutorials under "Applications".
- `hardroxide inspect` shows what a design will become before you generate;
  `hardroxide update` swaps in a rebuilt bitstream without touching the Rust
  source when the interface did not change.

## Status

Version 2.7. Validated on a **PYNQ-Z2** (PYNQ 3.1, kernel 6.6): bitstream
loading, PL clocks, GPIO, HLS (scalar and pointer arguments), DMA, Timer, IIC,
SPI (internal loopback), UART, BRAM, FIFO, interrupts (including through an AXI
interrupt controller), and a crate generated from the PYNQ base overlay. The
bare-metal and RTOS flavors (PS support, kernel, MMU and caches, DMA, a 10 ms control loop) on the same board and under
QEMU, and the applications listed above (HDMI, audio, SD, Ethernet, HTTPS to curl, OpenSSL and Edge). The results, and what is still unchecked, are in
[`docs/08-verification-gaps.md`](docs/08-verification-gaps.md).

Limits: Zynq-7000 only (UltraScale+ designs are refused with a clear error),
bare metal and RTOS on core 0 only (no SMP), direct-register-mode DMA only, one register block per
IP instance, and Vivado 2025.2 is the version actually exercised on hardware.

## Quick start

```
cargo install --path crates/hardroxide-cli      # installs `hardroxide`
hardroxide inspect design.xsa
hardroxide generate design.xsa -o my_design
```

then follow the [user guide](https://jwkunz.github.io/Hardroxide/book/guide/getting-started.html)
to cross-build for the board (`armv7-unknown-linux-gnueabihf`) and run
`board_check` first. For no operating system or the real-time kernel, generate with
`--platform baremetal` or `--platform rtos` and see
[Running bare metal](https://jwkunz.github.io/Hardroxide/book/guide/baremetal.html).

Before reprogramming the PL, stop anything on the board that uses it (a base
overlay's HDMI or GPIO drivers, for example): reprogramming under a live driver
can freeze the CPU. `load()` refuses when it detects one. See
[Running on hardware](https://jwkunz.github.io/Hardroxide/book/guide/hardware.html).

## License

Copyright (c) 2026 Numerius Engineering LLC. Licensed under either of
[Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at
your option. Contact: numerius.engineering@gmail.com.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
