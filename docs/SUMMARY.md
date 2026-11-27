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
- [Linux on Zynq UltraScale+ (preview)](guide/linux-ultrascale.md)

# Tutorials

- [Choosing a flavor](tutorials/index.md)

# Linux tutorials

- [Setup](tutorials/linux/index.md)
  - [1. LEDs, buttons and switches](tutorials/linux/01-lights.md)
  - [2. Interrupts: wait for a button](tutorials/linux/02-interrupts.md)
  - [3. Memory inside the FPGA](tutorials/linux/03-block-ram.md)
  - [4. The whole overlay](tutorials/linux/04-base-overlay.md)

# Bare-metal tutorials

- [Setup](tutorials/baremetal/index.md)
  - [1. Run over JTAG](tutorials/baremetal/01-jtag.md)
  - [2. A PL interrupt through the GIC](tutorials/baremetal/02-interrupt.md)

# RTOS tutorials

- [Setup](tutorials/rtos/index.md)
  - [1. A 10 ms control loop on the kernel](tutorials/rtos/01-control-loop.md)

# Application tutorials

- [Setup](tutorials/apps/index.md)
  - [1. Lights and buttons on the base overlay](tutorials/apps/01-base-lights.md)
  - [2. Pixels on the HDMI output](tutorials/apps/02-hdmi.md)
  - [3. Sound](tutorials/apps/03-audio.md)
  - [4. An SD card and files](tutorials/apps/04-sd.md)
  - [5. Ethernet and a web server](tutorials/apps/05-network.md)
  - [6. HTTPS with a self-signed certificate](tutorials/apps/06-https.md)
  - [7. Everything at once](tutorials/apps/07-integration.md)

# Your own design

- [Beyond the base overlay](tutorials/own-design/index.md)
  - [1. Timer, SPI and interrupts](tutorials/own-design/01-timer-spi-irq.md)
  - [2. UART and I2C without wires](tutorials/own-design/02-uart-iic-loopback.md)
  - [3. An HLS kernel](tutorials/own-design/03-hls-vadd.md)
  - [4. An HLS kernel that reads memory](tutorials/own-design/04-hls-pointers.md)
  - [5. AXI DMA](tutorials/own-design/05-dma.md)
  - [6. AXI4-Stream FIFO](tutorials/own-design/06-stream-fifo.md)
  - [7. Updating a bitstream](tutorials/own-design/07-update.md)
  - [8. A timer's interrupt on bare metal](tutorials/own-design/08-baremetal-timer.md)
  - [9. DMA on bare metal](tutorials/own-design/09-baremetal-dma.md)

# Zynq UltraScale+ (3.0 preview)

- [Setup (RFSoC 4x2)](tutorials/rfsoc/index.md)
  - [1. First light](tutorials/rfsoc/01-first-light.md)
  - [2. The base overlay](tutorials/rfsoc/02-base-overlay.md)
  - [3. The board's own hardware](tutorials/rfsoc/03-board-hardware.md)
  - [4. A mood light on the kernel](tutorials/rfsoc/04-mood-light.md)
  - [5. The board's hardware as tasks](tutorials/rfsoc/05-kernel-board.md)
  - [6. The kernel's tests and timing](tutorials/rfsoc/06-kernel-timing.md)
  - [7. Network and SD card](tutorials/rfsoc/07-network-sd.md)
  - [8. The base overlay under Linux](tutorials/rfsoc/08-linux.md)
  - [9. A web page for the board](tutorials/rfsoc/09-linux-web.md)
  - [10. The FPGA's own memory](tutorials/rfsoc/10-linux-memory.md)
  - [11. A tone out and back](tutorials/rfsoc/11-radio.md)

# Reference

- [What each library can and cannot do](reference/libraries.md)
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
- [hardroxide-psu](reference/psu.md)
- [hardroxide-board](reference/board.md)
- [hardroxide-rt](reference/rt.md)
- [hardroxide-video](reference/video.md)
- [hardroxide-gfx](reference/gfx.md)
- [hardroxide-audio](reference/audio.md)
- [hardroxide-fat](reference/fat.md)
- [hardroxide-net](reference/net.md)
- [hardroxide-crypto](reference/crypto.md)
- [hardroxide-tls](reference/tls.md)
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
  - [Audio codec controller](reference/ip-audio.md)
  - [AXI VDMA](reference/ip-vdma.md)
  - [Video Timing Controller](reference/ip-vtc.md)
  - [Dynamic clock generator](reference/ip-dynclk.md)
  - [RF data converter](reference/ip-rfdc.md)
  - [RFSoC radio blocks](reference/ip-radio.md)
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
- [ADR 0005: Zynq UltraScale+ (3.0)](decisions/0005-ultrascale-plus.md)
- [ADR 0006: The radio driver (3.1)](decisions/0006-radio.md)

# Status

- [HWH findings](07-hwh-findings.md)
- [Verification gaps](08-verification-gaps.md)
