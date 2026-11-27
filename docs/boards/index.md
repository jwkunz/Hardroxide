# The boards

Hardroxide has been run on two boards, both made for teaching and research with PYNQ, AMD's Python framework. Each has
its own chapter here, its own [tutorials](../tutorials/index.md) and its own [examples](../examples.md).

| | [PYNQ-Z2](pynq-z2.md) | [RFSoC 4x2](rfsoc-4x2.md) |
|---|---|---|
| Family | Zynq-7000 | Zynq UltraScale+ RFSoC |
| Chip | XC7Z020 | XCZU48DR |
| Processor | 2 x Arm Cortex-A9 (32-bit) | 4 x Arm Cortex-A53 (64-bit), 2 x Cortex-R5F |
| Memory | 512 MB DDR3 | 4 GB DDR4 for the processor, 4 GB for the FPGA |
| Good for learning | FPGA basics, video, audio, networking, real-time control | the same on a 64-bit processor, and **radio**: very fast converters |
| Linux image used | PYNQ 3.1 | RFSoC-PYNQ 4.0.0 |
| Rust targets | `armv7-unknown-linux-gnueabihf`, `armv7a-none-eabihf` | `aarch64-unknown-linux-gnu`, `aarch64-unknown-none` |
| Tutorials | [Linux](../tutorials/linux/index.md), [bare metal](../tutorials/baremetal/index.md), [RTOS](../tutorials/rtos/index.md), [applications](../tutorials/apps/index.md), [your own design](../tutorials/own-design/index.md) | [RFSoC tutorials](../tutorials/rfsoc/index.md) |
| Examples | [PYNQ-Z2](../examples.md#pynq-z2) | [RFSoC 4x2](../examples.md#rfsoc-4x2) |

## What is the same on both

- One tool and one generated API: `hardroxide generate` writes a crate for Linux, bare metal or the real-time kernel,
  with the same drivers.
- The **base overlay**, the FPGA design each board runs under PYNQ, is what the tutorials use. You need no Vivado to
  follow them.
- For Linux you need the board's PYNQ SD card and a network connection to it. For bare metal and the kernel you need a
  USB cable and the AMD **Vitis** tools (2025.2 was used), which load programs over JTAG.

> **Your SD card.** Some examples write to or erase a card. They do so only to a card you name by its serial number
> when building them. Keep it that way in your own programs.

## Other boards

Only these two have been tried. The generator and drivers are not written for one board: a design for another Zynq-7000
board should generate and build, and one for another Zynq UltraScale+ part should too, but nothing has run there, the
ready-made examples assume these two boards' base overlays, and the bare-metal support makes assumptions (memory size,
console pins) that you would need to check. What has been verified, and how, is in
[Verification gaps](../08-verification-gaps.md).
