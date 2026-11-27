# PYNQ-Z2: your own design

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** Linux and bare metal

The tutorials before these run on the board's base overlay, which you did not have to build. These show what you do when
the FPGA design is **yours**. Each has a small design made for one purpose, in `fixtures/`: its own bitstream, its own
generated crate, the same steps you will take with a design you export from Vivado
([how](../../guide/vivado.md)). Nothing needs wires.

Do the first [base overlay tutorials](../linux/index.md) first: these assume the board is set up
([Linux setup](../linux/index.md#before-you-start-once), or for 8 and 9 the
[bare-metal host setup](../baremetal/index.md#host-setup)) and that you know the generate, build and run loop.

| # | Tutorial | Design (`fixtures/`) | Flavor | You learn |
|---|---|---|---|---|
| 1 | [Timer, SPI and interrupts](01-timer-spi-irq.md) | `pynq_z2_peripherals.xsa` | Linux | waiting for an interrupt; applying the overlay |
| 2 | [UART and I2C without wires](02-uart-iic-loopback.md) | `pynq_z2_loopback.xsa` | Linux | two cores wired together inside the FPGA |
| 3 | [An HLS kernel](03-hls-vadd.md) | `pynq_z2_hls_vadd.xsa` | Linux | typed registers for a Vitis HLS kernel |
| 4 | [An HLS kernel that reads memory](04-hls-pointers.md) | `pynq_z2_hls_scale.xsa` | Linux | pointer arguments and DMA buffers |
| 5 | [AXI DMA](05-dma.md) | `pynq_z2_dma_loopback.xsa` | Linux | moving data with ownership-checked buffers |
| 6 | [AXI4-Stream FIFO](06-stream-fifo.md) | `pynq_z2_fifo.xsa` | Linux | packets, and why lengths round up |
| 7 | [Updating a bitstream](07-update.md) | `variants/` | Linux | swapping a rebuilt design without touching code |
| 8 | [A timer's interrupt](08-baremetal-timer.md) | `board/pynq_z2_peripherals.xsa` | bare metal | a periodic PL interrupt, measured |
| 9 | [DMA with the caches on](09-baremetal-dma.md) | `board/pynq_z2_dma_loopback.xsa` | bare metal | `.dma` buffers; DMA speed against CPU speed |

**Under Linux, one design per boot** if their interrupt overlays differ: shut the board down and switch it off and on
between them ([why](../../guide/hardware.md#troubleshooting)). Interrupts that these designs put on the first PL line
(`IRQ_F2P[0]`) work when level-triggered; an edge-triggered one there gets nothing on the PYNQ image
([the table](../../guide/hardware.md#troubleshooting)).
