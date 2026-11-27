# PYNQ-Z2

A Zynq-7000 board made by TUL for teaching with PYNQ, and the first board Hardroxide supported. It has something to see
or hear for almost every lesson: lights, buttons, HDMI, audio, a card slot and Ethernet.

## At a glance

| | |
|---|---|
| Chip | Zynq-7000 `XC7Z020-1CLG400C` |
| Processor | two Arm Cortex-A9 cores, 650 MHz. Linux uses both; bare metal and the kernel use one |
| Memory | 512 MB DDR3 |
| FPGA design the tutorials use | the PYNQ base overlay, `fixtures/board/pynq_base_overlay.xsa` ([what is in it](../reference/base-overlay.md#pynq-z2)) |
| Linux image used | PYNQ 3.1 (kernel 6.6), on a microSD card of at least 8 GB |

Vendor pages: [the board in PYNQ's documentation](https://pynq.readthedocs.io/en/v3.1/pynq_overlays/pynqz2.html)
(which links to TUL's manual and schematics),
[setting up the image](https://pynq.readthedocs.io/en/v3.1/getting_started/pynq_z2_setup.html),
[image downloads](http://www.pynq.io/boards.html).

## What is on it, and what Hardroxide does with it

| On the board | With Hardroxide | See |
|---|---|---|
| 4 LEDs, 2 colour LEDs, 4 buttons, 2 switches | yes | [Linux 1](../tutorials/linux/01-lights.md), [Bare metal 1](../tutorials/baremetal/01-jtag.md) |
| HDMI output | yes, 800x600 to 1920x1080 (the tested monitor rejects 640x480) | [Applications 2](../tutorials/apps/02-hdmi.md) |
| HDMI input | no | |
| Audio: headphone output, line input | yes | [Applications 3](../tutorials/apps/03-audio.md) |
| Audio: microphone input | not heard working | |
| microSD slot | yes: SD driver and FAT32 | [Applications 4](../tutorials/apps/04-sd.md) |
| Gigabit Ethernet | yes: TCP/IP, a web server, HTTPS | [Applications 5](../tutorials/apps/05-network.md) |
| USB-UART and JTAG | yes: console, and loading programs | [Running bare metal](../guide/baremetal.md) |
| FPGA with AXI peripherals, DMA, HLS kernels | yes | [Your own design](../tutorials/own-design/index.md) |
| USB host, Pmod, Arduino and Raspberry Pi connectors | no drivers of their own; reachable by an FPGA design you make | |

## What you need

| For | Besides the board |
|---|---|
| Linux | the PYNQ card, an Ethernet cable, power |
| Bare metal and the kernel | a micro-USB cable (the port marked `PROG UART`) and the Vitis tools |
| Video | an HDMI monitor |
| Audio | headphones; for the self-tests a 3.5 mm cable from `HPH OUT` to `LINE IN` |
| Network | an Ethernet cable to your computer or a router |
| SD card | a card you are willing to have written or erased |

## Setting it up

- **Linux:** set the boot jumper to SD and follow [the Linux setup](../tutorials/linux/index.md#before-you-start-once).
  Once per boot, free the FPGA from the image's own drivers. Shut the board down and switch it off rather than
  `reboot` it.
- **Bare metal and the kernel:** set the boot jumper to JTAG and follow
  [the bare-metal setup](../tutorials/baremetal/index.md). The console is usually `/dev/ttyUSB1`.

## Where to go

[Tutorials](../tutorials/index.md) · [Examples for this board](../examples.md#pynq-z2) ·
[Board results](../08-verification-gaps.md#board-results)
