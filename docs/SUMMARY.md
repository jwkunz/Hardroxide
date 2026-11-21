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
- [Running bare metal and RTOS](guide/baremetal.md)

# Tutorials

- [Choosing a flavor](tutorials/index.md)

# Linux tutorials

- [Setup](tutorials/linux/index.md)
  - [1. LEDs, buttons and switches](tutorials/linux/01-gpio.md)
  - [2. Timer, SPI and interrupts](tutorials/linux/02-timer-spi-irq.md)
  - [3. UART and I2C without wires](tutorials/linux/03-uart-iic-loopback.md)
  - [4. An HLS kernel](tutorials/linux/04-hls-vadd.md)
  - [5. An HLS kernel that reads memory](tutorials/linux/05-hls-pointers.md)
  - [6. AXI DMA](tutorials/linux/06-dma.md)
  - [7. Many interrupts, one controller](tutorials/linux/07-interrupt-controller.md)
  - [8. Block RAM and UART](tutorials/linux/08-bram-uart.md)
  - [9. AXI4-Stream FIFO](tutorials/linux/09-stream-fifo.md)
  - [10. Updating a bitstream](tutorials/linux/10-update.md)
  - [11. A design you did not build](tutorials/linux/11-base-overlay.md)

# Bare-metal tutorials

- [Setup](tutorials/baremetal/index.md)
  - [1. Run over JTAG](tutorials/baremetal/01-jtag.md)
  - [2. A PL interrupt through the GIC](tutorials/baremetal/02-timer-interrupt.md)
  - [3. DMA with the caches on](tutorials/baremetal/03-dma.md)

# RTOS tutorials

- [Setup](tutorials/rtos/index.md)
  - [1. A 10 ms control loop on the kernel](tutorials/rtos/01-control-loop.md)

# Application tutorials

- [Setup](tutorials/apps/index.md)
  - [1. Lights and buttons on the base overlay](tutorials/apps/01-base-lights.md)
  - [2. Pixels on the HDMI output](tutorials/apps/02-hdmi.md)

# Reference

- [CLI](reference/cli.md)
- [Generated crate](reference/generated-crate.md)
- [Bare-metal crate](reference/baremetal.md)
- [PYNQ base overlay](reference/base-overlay.md)
- [hardroxide-hwh](reference/hwh.md)
- [hardroxide-gen](reference/gen.md)
- [hardroxide-mmio](reference/mmio.md)
- [hardroxide-fpga](reference/fpga.md)
- [hardroxide-dma](reference/dma.md)
- [hardroxide-irq](reference/irq.md)
- [hardroxide-ps7](reference/ps7.md)
- [hardroxide-rt](reference/rt.md)
- [hardroxide-video](reference/video.md)
- [hardroxide-gfx](reference/gfx.md)
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
  - [AXI VDMA](reference/ip-vdma.md)
  - [Video Timing Controller](reference/ip-vtc.md)
  - [Dynamic clock generator](reference/ip-dynclk.md)
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
- [ADR 0003: Bare-metal and RTOS platforms](decisions/0003-bare-metal-and-rtos-platforms.md)
- [ADR 0004: Base overlay applications (2.1 to 2.7)](decisions/0004-base-overlay-applications.md)

# Status

- [HWH findings](07-hwh-findings.md)
- [Verification gaps](08-verification-gaps.md)
