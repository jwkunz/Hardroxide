# RFSoC 4x2

A Zynq UltraScale+ **RFSoC** board developed by the PYNQ team with Real Digital: a 64-bit processor, an FPGA, and radio
converters fast enough to produce and sample signals directly at hundreds of megahertz and above. Its vendors offer it
to universities and research institutes (see their pages for the terms). Support for it is new in Hardroxide 3.0.

## At a glance

| | |
|---|---|
| Chip | Zynq UltraScale+ RFSoC `XCZU48DR` (see [speed grade](#speed-grade)) |
| Processor | four Arm Cortex-A53 cores (1.2 GHz as set up here) and two Cortex-R5F. Linux uses the A53 cores; bare metal and the kernel use one; the R5F cores are not used |
| Memory | 4 GB DDR4 for the processor, 4 GB DDR4 for the FPGA |
| Radio | 4 inputs (ADC, up to 5 GS/s) and 2 outputs (DAC, up to 9.85 GS/s) on SMA connectors; the base overlay runs them at 4.9152 GS/s |
| FPGA design the tutorials use | RFSoC-PYNQ's base overlay, `fixtures/board/rfsoc4x2_base_overlay.xsa` ([what is in it](../reference/base-overlay.md#rfsoc-4x2)) |
| Linux image used | RFSoC-PYNQ 4.0.0, on the board's microSD card |

Vendor pages: [RFSoC-PYNQ's overview](https://www.rfsoc-pynq.io/rfsoc_4x2_overview.html),
[Real Digital's board page](https://www.realdigital.org/hardware/rfsoc-4x2),
[image downloads](http://www.pynq.io/boards.html).

## What is on it, and what Hardroxide does with it

| On the board | With Hardroxide | See |
|---|---|---|
| LEDs, colour LEDs, buttons, switches | yes | [RFSoC 2](../tutorials/rfsoc/02-base-overlay.md) |
| Radio inputs and outputs | yes: **tones** out of the DACs (frequency, phase, amplitude), records from the ADCs. No arbitrary waveforms, no common start of sending and recording, no change of sample rate | [RFSoC 11](../tutorials/rfsoc/11-radio.md) |
| Radio clock chips | yes: programmed from register files | [RFSoC 3](../tutorials/rfsoc/03-board-hardware.md) |
| OLED display, power monitors, identity memory, temperature sensors | yes | [RFSoC 3](../tutorials/rfsoc/03-board-hardware.md) |
| Gigabit Ethernet | yes | [RFSoC 7](../tutorials/rfsoc/07-network-sd.md) |
| microSD slot | yes: SD driver and FAT32 | [RFSoC 7](../tutorials/rfsoc/07-network-sd.md#the-sd-card) |
| USB-UART and JTAG | yes: console, and loading programs | [RFSoC 1](../tutorials/rfsoc/01-first-light.md) |
| The FPGA's own DDR4 | yes, from Linux | [RFSoC 10](../tutorials/rfsoc/10-linux-memory.md) |
| FPGA with AXI peripherals, DMA, HLS kernels | yes | [examples](../examples.md#rfsoc-4x2-other-designs) |
| QSFP28 (100 Gb Ethernet), Mini DisplayPort, USB 3, SYZYGY, Pmod | no | |

## Two things to know before touching the radio or the FPGA's memory

1. **The clock chips forget at power-off.** Until they are programmed, an access to the radio blocks or the FPGA's
   memory never completes, and the board has to be switched off. The PYNQ image programs them while starting; the
   bare-metal radio programs do it themselves.
2. **Cable only the board's own DACs to its ADCs**, unless you have checked the board's documentation for what the
   inputs bear.

## What you need

| For | Besides the board |
|---|---|
| Linux | the PYNQ card, an Ethernet cable, the 12 V supply |
| Bare metal and the kernel | the supply, a USB cable (the port marked `PROG UART`) and the Vitis tools |
| Radio | two SMA cables: DAC A to ADC A, DAC B to ADC B |
| Network | an Ethernet cable to your computer or a network |
| SD card programs | a card you are willing to have erased |

## Setting it up

- **Linux:** boot mode switch on SD; **wait about two minutes** after switching on before loading a design
  ([RFSoC 8](../tutorials/rfsoc/08-linux.md)).
- **Bare metal and the kernel:** boot mode switch on JTAG ([setup](../tutorials/rfsoc/index.md#board-setup)). The
  console is usually `/dev/ttyUSB1`.
- **Your own FPGA designs** for this chip need a Vivado license that covers it. The tutorials need none.

## Speed grade

The sources disagree: the board files Real Digital publishes say speed grade -2, while the reference manual and
RFSoC-PYNQ's overview list -1. Vivado rejects the board's processor settings for -1, so this repository's designs are
built for -2 and also checked to meet timing at -1.

## Where to go

[Tutorials](../tutorials/rfsoc/index.md) · [Examples for this board](../examples.md#rfsoc-4x2) ·
[Results](../decisions/0005-ultrascale-plus.md)
