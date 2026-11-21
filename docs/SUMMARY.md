# Summary

[Introduction](introduction.md)

# User Guide

- [The workflow](guide/workflow.md)
- [Getting started](guide/getting-started.md)
- [Exporting an XSA from Vivado](guide/vivado.md)
- [How it works](guide/concepts.md)
- [Generating a crate](guide/generating.md)
- [Using a generated crate](guide/using.md)
- [Interrupts](guide/interrupts.md)
- [Updating a bitstream](guide/updating.md)
- [Running on hardware](guide/hardware.md)

# Tutorials

- [Overview and setup](tutorials/index.md)
  - [1. LEDs, buttons and switches](tutorials/01-gpio.md)
  - [2. Timer, SPI and interrupts](tutorials/02-timer-spi-irq.md)
  - [3. UART and I2C without wires](tutorials/03-uart-iic-loopback.md)
  - [4. An HLS kernel](tutorials/04-hls-vadd.md)
  - [5. An HLS kernel that reads memory](tutorials/05-hls-pointers.md)
  - [6. AXI DMA](tutorials/06-dma.md)
  - [7. Many interrupts, one controller](tutorials/07-interrupt-controller.md)
  - [8. Block RAM and UART](tutorials/08-bram-uart.md)
  - [9. AXI4-Stream FIFO](tutorials/09-stream-fifo.md)
  - [10. Updating a bitstream](tutorials/10-update.md)
  - [11. A design you did not build](tutorials/11-base-overlay.md)

# Reference

- [CLI](reference/cli.md)
- [Generated crate](reference/generated-crate.md)
- [hardroxide-hwh](reference/hwh.md)
- [hardroxide-gen](reference/gen.md)
- [hardroxide-mmio](reference/mmio.md)
- [hardroxide-fpga](reference/fpga.md)
- [hardroxide-dma](reference/dma.md)
- [hardroxide-irq](reference/irq.md)
- [hardroxide-ip](reference/ip.md)
  - [AXI GPIO](reference/ip-gpio.md)
  - [HLS IP](reference/ip-hls.md)
  - [AXI DMA](reference/ip-dma.md)
  - [AXI Timer](reference/ip-timer.md)
  - [AXI Interrupt Controller](reference/ip-intc.md)
  - [AXI BRAM Controller](reference/ip-bram.md)
  - [AXI UART Lite](reference/ip-uart.md)
  - [AXI4-Stream FIFO](reference/ip-fifo.md)
  - [AXI IIC](reference/ip-iic.md)
  - [AXI Quad SPI](reference/ip-spi.md)
- [Board scripts](reference/scripts.md)
- [Fixtures](reference/fixtures.md)

# Design

- [Design index](design-index.md)
- [Vision](00-vision.md)
- [Requirements](01-requirements.md)
- [Architecture](02-architecture.md)
- [Generated crate (design)](03-generated-crate.md)
- [Runtime](04-runtime.md)
- [Roadmap](05-roadmap.md)
- [Session handoff](06-session-handoff.md)
- [Releasing](release.md)
- [ADR 0001: MMIO and DMA model](decisions/0001-mmio-and-dma-memory-model.md)
- [ADR 0002: Bitstream embedding and updates](decisions/0002-bitstream-embedding-and-updates.md)
- [ADR 0003: Bare-metal and RTOS platforms (proposed)](decisions/0003-bare-metal-and-rtos-platforms.md)

# Status

- [HWH findings](07-hwh-findings.md)
- [Verification gaps](08-verification-gaps.md)
