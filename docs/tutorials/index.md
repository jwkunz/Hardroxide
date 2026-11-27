# Tutorials

## Which board do you have?

| Board | Tutorials | Also |
|---|---|---|
| [**PYNQ-Z2**](../boards/pynq-z2.md) (Zynq-7000) | the pages below: [Linux](linux/index.md), [bare metal](baremetal/index.md), [RTOS](rtos/index.md), [applications](apps/index.md), [your own design](own-design/index.md) | [every example for it](../examples.md#pynq-z2) |
| [**RFSoC 4x2**](../boards/rfsoc-4x2.md) (Zynq UltraScale+) | [the RFSoC tutorials](rfsoc/index.md): Linux, bare metal, the real-time kernel and the radio | [every example for it](../examples.md#rfsoc-4x2) |

Every tutorial page names its board and flavor at the top.

## Choosing a flavor

The tutorials are grouped by **flavor**, that is by what runs on the board's processor. Pick the one that
matches how you will deploy; all three start from the same `.xsa` and the same generated API
(`Design`, typed drivers, `pac`).

| Flavor | `--platform` | Your program is | Start with |
|---|---|---|---|
| [**Linux**](linux/index.md) | `linux` (default) | a Linux process on the stock PYNQ image; Linux owns the PS peripherals, Hardroxide manages the PL | [Linux 1: LEDs, buttons and switches](linux/01-lights.md) |
| [**Bare metal**](baremetal/index.md) | `baremetal` | the only software on core 0: startup, interrupts, timers, UART and caches come from `hardroxide-ps7`; started over JTAG or from SD | [Bare metal 1: Run over JTAG](baremetal/01-jtag.md) |
| [**RTOS**](rtos/index.md) | `rtos` | bare metal plus the `hardroxide-rt` kernel: prioritised tasks, semaphores, mutexes, queues, 10 ms control loops | [RTOS 1: A 10 ms control loop](rtos/01-control-loop.md) |

**Applications** ([setup](apps/index.md)) are programs that do something worth showing on the PYNQ base overlay, built with the bare-metal and RTOS flavors.

Not sure which? Use Linux when you want the operating system's file system, network and tools next to your
FPGA design. Use bare metal for the smallest and most direct software on the processor. Use the RTOS when
several activities with deadlines must share the processor.

You do **not** need Vivado for any of them: the `.xsa` files are in `fixtures/`. The output shown was
captured on a PYNQ-Z2. See [Choosing a flavor](../guide/workflow.md#choosing-a-flavor) for the trade-offs.

## The PYNQ-Z2 tutorials

**Start on the board's base overlay.** The first tutorials of every flavor, and the applications, use the one FPGA
design your board already has: the PYNQ base overlay (`fixtures/board/pynq_base_overlay.xsa`). Then
[your own design](own-design/index.md) goes through what that overlay does not have.

**Linux** ([setup](linux/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [LEDs, buttons and switches](linux/01-lights.md) | the whole workflow, end to end |
| 2 | [Interrupts: wait for a button](linux/02-interrupts.md) | the interrupt overlay and controller |
| 3 | [Memory inside the FPGA](linux/03-block-ram.md) | block RAM as words and bytes |
| 4 | [The whole overlay](linux/04-base-overlay.md) | a large design you did not build |

**Bare metal** ([setup](baremetal/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Run over JTAG](baremetal/01-jtag.md) | no operating system: `--platform baremetal`, `hardroxide run` |
| 2 | [A PL interrupt through the GIC](baremetal/02-interrupt.md) | `IrqWait`, handler-free interrupts, waiting for a button |

**RTOS** ([setup](rtos/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [A 10 ms control loop on the kernel](rtos/01-control-loop.md) | tasks, priorities and a periodic release with `--platform rtos` |

**Applications** ([setup](apps/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Lights and buttons on the base overlay](apps/01-base-lights.md) | the PYNQ base overlay without an OS; interrupts through the controller; `IrqEvent`; a mood light |
| 2 | [Pixels on the HDMI output](apps/02-hdmi.md) | HDMI output and a graphics library |
| 3 | [Sound](apps/03-audio.md) | codec, WAV playback, synthesis, recording, a loopback self-test |

**Your own design** ([overview](own-design/index.md)): small designs made for one purpose each.

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Timer, SPI and interrupts](own-design/01-timer-spi-irq.md) | waiting for an interrupt |
| 2 | [UART and I2C without wires](own-design/02-uart-iic-loopback.md) | two cores wired together inside the FPGA |
| 3 | [An HLS kernel](own-design/03-hls-vadd.md) | typed registers for a Vitis HLS kernel |
| 4 | [An HLS kernel that reads memory](own-design/04-hls-pointers.md) | pointer arguments and DMA buffers |
| 5 | [AXI DMA](own-design/05-dma.md) | moving data with ownership-checked buffers |
| 6 | [AXI4-Stream FIFO](own-design/06-stream-fifo.md) | packets, and why lengths round up |
| 7 | [Updating a bitstream](own-design/07-update.md) | swapping a rebuilt design without touching code |
| 8 | [A timer's interrupt on bare metal](own-design/08-baremetal-timer.md) | a periodic PL interrupt, measured |
| 9 | [DMA on bare metal](own-design/09-baremetal-dma.md) | non-cacheable `.dma` buffers; DMA speed against CPU speed |
