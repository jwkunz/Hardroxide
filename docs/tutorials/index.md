# Tutorials

The tutorials are grouped by **flavor**, that is by what runs on the board's processor. Pick the one that
matches how you will deploy; all three start from the same `.xsa` and the same generated API
(`Design`, typed drivers, `pac`).

| Flavor | `--platform` | Your program is | Start with |
|---|---|---|---|
| [**Linux**](linux/index.md) | `linux` (default) | a Linux process on the stock PYNQ image; Linux owns the PS peripherals, Hardroxide manages the PL | [Linux 1: LEDs, buttons and switches](linux/01-gpio.md) |
| [**Bare metal**](baremetal/index.md) | `baremetal` | the only software on core 0: startup, interrupts, timers, UART and caches come from `hardroxide-ps7`; started over JTAG or from SD | [Bare metal 1: Run over JTAG](baremetal/01-jtag.md) |
| [**RTOS**](rtos/index.md) | `rtos` | bare metal plus the `hardroxide-rt` kernel: prioritised tasks, semaphores, mutexes, queues, 10 ms control loops | [RTOS 1: A 10 ms control loop](rtos/01-control-loop.md) |

**Applications** ([setup](apps/index.md)) are programs that do something worth showing on the PYNQ base overlay, built with the bare-metal and RTOS flavors.

Not sure which? Use Linux when you want the operating system's file system, network and tools next to your
FPGA design. Use bare metal for the smallest and most direct software on the processor. Use the RTOS when
several activities with deadlines must share the processor.

You do **not** need Vivado for any of them: the `.xsa` files are in `fixtures/`. The output shown was
captured on a PYNQ-Z2. See [Choosing a flavor](../guide/workflow.md#choosing-a-flavor) for the trade-offs.

## The tutorials

**Linux** ([setup](linux/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [LEDs, buttons and switches](linux/01-gpio.md) | the whole workflow, end to end |
| 2 | [Timer, SPI and interrupts](linux/02-timer-spi-irq.md) | waiting for an interrupt; applying the overlay |
| 3 | [UART and I2C without wires](linux/03-uart-iic-loopback.md) | two cores wired together inside the FPGA |
| 4 | [An HLS kernel](linux/04-hls-vadd.md) | typed registers for a Vitis HLS kernel |
| 5 | [An HLS kernel that reads memory](linux/05-hls-pointers.md) | pointer arguments and DMA buffers |
| 6 | [AXI DMA](linux/06-dma.md) | moving data with ownership-checked buffers |
| 7 | [Many interrupts, one controller](linux/07-interrupt-controller.md) | AXI interrupt controller fan-in |
| 8 | [Block RAM and UART](linux/08-bram-uart.md) | memory as a byte buffer |
| 9 | [AXI4-Stream FIFO](linux/09-stream-fifo.md) | packets, and why lengths round up |
| 10 | [Updating a bitstream](linux/10-update.md) | swapping a rebuilt design without touching code |
| 11 | [A design you did not build](linux/11-base-overlay.md) | the PYNQ base overlay: 70 IP, one command |

**Bare metal** ([setup](baremetal/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Run over JTAG](baremetal/01-jtag.md) | no operating system: `--platform baremetal`, `hardroxide run` |
| 2 | [A PL interrupt through the GIC](baremetal/02-timer-interrupt.md) | `IrqWait`, handler-free interrupts, a periodic interrupt measured |
| 3 | [DMA with the caches on](baremetal/03-dma.md) | non-cacheable `.dma` buffers; DMA speed against CPU speed |

**RTOS** ([setup](rtos/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [A 10 ms control loop on the kernel](rtos/01-control-loop.md) | tasks, priorities and a periodic release with `--platform rtos` |

**Applications** ([setup](apps/index.md))

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Lights and buttons on the base overlay](apps/01-base-lights.md) | the PYNQ base overlay without an OS; interrupts through the controller; `IrqEvent`; a mood light |
| 2 | [Pixels on the HDMI output](apps/02-hdmi.md) | HDMI output and a graphics library (**not yet run on a board**) |
| 3 | [Sound](apps/03-audio.md) | codec, WAV playback, synthesis, recording (**not yet run on a board**) |
