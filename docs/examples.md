# Example projects

Every example program in the repository, by board and then by flavor. Pick a row, copy the program into a generated crate
and run it. Most use the **base overlay**, the FPGA design each board already ships with under PYNQ, so you need no
Vivado to run them.

| Board | Linux | Bare metal | RTOS | Processor and kernel | Other designs |
|---|---|---|---|---|---|
| **PYNQ-Z2** (Zynq-7000) | [Linux](#pynq-z2-linux) | [bare metal](#pynq-z2-bare-metal) | [RTOS](#pynq-z2-rtos) | [programs](#pynq-z2-processor-and-kernel) | [designs](#pynq-z2-other-designs) |
| **RFSoC 4x2** (Zynq UltraScale+) | [Linux](#rfsoc-4x2-linux) | [bare metal](#rfsoc-4x2-bare-metal) | [RTOS](#rfsoc-4x2-rtos) | [programs](#rfsoc-4x2-processor-and-kernel) | [designs](#rfsoc-4x2-other-designs) |

New to a board? Start with [Linux 1](tutorials/linux/01-lights.md) or [Bare metal 1](tutorials/baremetal/01-jtag.md) on
the PYNQ-Z2, and with [RFSoC 1](tutorials/rfsoc/01-first-light.md) on the RFSoC 4x2.

**Reading the tables.** Each program has been run on its board unless its row says otherwise
([results](08-verification-gaps.md)). *Also needs* lists what goes beyond the generated crate and the board: libraries
to add (`net` means `hardroxide-net`, and so on), files, settings and hardware. All commands start at the root of a
checkout of this repository, with the `hardroxide` tool installed ([Getting started](guide/getting-started.md)).

Besides these, **every generated crate comes with checks of its own** (`board_check`, then one per kind of block):
[Linux](reference/generated-crate.md#examples), [bare metal and RTOS](reference/baremetal.md).

## PYNQ-Z2

The base overlay is `fixtures/board/pynq_base_overlay.xsa` ([what is in it](reference/base-overlay.md#pynq-z2)).

### PYNQ-Z2: Linux

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --name design_1 -o work/z2-linux --hardroxide-path .
$ cp apps/base/linux/lights.rs work/z2-linux/examples/
$ scripts/board/run.sh work/z2-linux lights
```

Prepare the board once per boot ([setup](tutorials/linux/index.md#before-you-start-once)); an *interrupt overlay* is
applied as in [Linux 2](tutorials/linux/02-interrupts.md).

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`lights`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/linux/lights.rs) | the LEDs, buttons and switches | — | [Linux 1](tutorials/linux/01-lights.md) |
| [`interrupts`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/linux/interrupts.rs) | wait for a button as a UIO interrupt | the interrupt overlay | [Linux 2](tutorials/linux/02-interrupts.md) |
| [`block_ram`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/linux/block_ram.rs) | memory inside the FPGA, as words and bytes | — | [Linux 3](tutorials/linux/03-block-ram.md) |
| [`base_tour`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/base/base_tour.rs) | a tour: switches, buttons, LEDs, the interrupt controller | generate the crate with `--name base` | — |

### PYNQ-Z2: bare metal

The board in JTAG boot mode ([setup](tutorials/baremetal/index.md)):

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal --name design_1 -o work/z2-bm --hardroxide-path .
$ cp apps/base/baremetal/leds.rs work/z2-bm/examples/
$ cd work/z2-bm
$ cargo build --release --example leds
$ hardroxide run target/armv7a-none-eabihf/release/examples/leds --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 30
```

<a id="adding-what-a-program-needs"></a>**Adding what a program needs**, inside the crate, before building:

| *Also needs* says | Do |
|---|---|
| `gfx`, `fat`, `net`, `crypto`, `tls` | `cargo add --dev --path ../../crates/hardroxide-net` (with that name) |
| `audio`, `video` | the same, with `--no-default-features` |
| feature `net` | `sed -i 's/"panic-handler"\]/"panic-handler", "net"]/' Cargo.toml` |
| `chime.wav` | `cp ../../apps/base/data/chime.wav examples/` |
| the test certificate | `cp ../../fixtures/tls/test-cert.der ../../fixtures/tls/test-key.bin examples/` (a public key: [read this](reference/tls.md#security-statement)) |
| `HRX_..._SERIAL` | build with the serial number of the card that may be written, as `sd_info` prints it |

#### Lights and buttons

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`leds`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/leds.rs) | LEDs, buttons and switches with no operating system | — | [Bare metal 1](tutorials/baremetal/01-jtag.md) |
| [`button_irq`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/button_irq.rs) | a button's interrupt through the interrupt controller to the processor | — | [Bare metal 2](tutorials/baremetal/02-interrupt.md) |
| [`lights`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/lights.rs) | count on the LEDs with the buttons, by interrupt | — | [Applications 1](tutorials/apps/01-base-lights.md) |
| [`selftest`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/selftest.rs) | checks clocks, LEDs and the interrupt path by itself | — | [Applications 1](tutorials/apps/01-base-lights.md) |

#### HDMI

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`video_demo`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/video_demo.rs) | a test pattern, then a 60 frames-per-second animation | `gfx`, `video`; HDMI monitor | [Applications 2](tutorials/apps/02-hdmi.md) |
| [`video_modes`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/video_modes.rs) | every display mode in turn (the tested monitor rejects 640x480) | `gfx`, `video`; HDMI monitor | [Applications 2](tutorials/apps/02-hdmi.md) |

#### Audio

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`audio_tone`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/audio_tone.rs) | a scale and a chord | `audio`; headphones | [Applications 3](tutorials/apps/03-audio.md) |
| [`audio_wav`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/audio_wav.rs) | a WAV file built into the program | `audio`; `chime.wav`; headphones | [Applications 3](tutorials/apps/03-audio.md) |
| [`audio_loopback_test`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/audio_loopback_test.rs) | self-test: a tone out and back | `audio`; cable HPH OUT to LINE IN | [Applications 3](tutorials/apps/03-audio.md) |
| [`audio_record_test`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/audio_record_test.rs) | self-test: record, play back, record again | `audio`; cable HPH OUT to LINE IN | [Applications 3](tutorials/apps/03-audio.md) |
| [`audio_record`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/audio_record.rs) | record five seconds and play them back (**not yet run on a board**) | `audio`; a source on LINE IN, headphones | [Applications 3](tutorials/apps/03-audio.md) |
| [`audio_loopback`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/audio_loopback.rs) | line in to headphones through the processor (**not yet run on a board**) | `audio`; a source on LINE IN, headphones | [Applications 3](tutorials/apps/03-audio.md) |

#### SD card

> **A program writes only to a card you name** by its serial number when building. That rule is newer than these
> programs' runs on a PYNQ-Z2: it has been tried under emulation only.

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`sd_info`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/sd_info.rs) | identify the card and list its files; writes nothing | `fat`; a card | [Applications 4](tutorials/apps/04-sd.md) |
| [`sd_erase_and_test`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/sd_erase_and_test.rs) | **erases the named card**, formats and tests it | `fat`; `HRX_ERASE_SERIAL` | [Applications 4](tutorials/apps/04-sd.md) |
| [`sd_logger`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/sd_logger.rs) | append timestamped lines to a file | `fat`; `HRX_WRITE_SERIAL` | [Applications 4](tutorials/apps/04-sd.md) |
| [`sd_wav`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/sd_wav.rs) | play a WAV file from the card | `audio`, `fat`; `chime.wav`; `HRX_WRITE_SERIAL` if the card has none; headphones | [Applications 4](tutorials/apps/04-sd.md) |
| [`sd_bmp`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/sd_bmp.rs) | save the screen to the card as a picture and show it again | `gfx`, `video`, `fat`; `HRX_WRITE_SERIAL`; HDMI monitor | [Applications 4](tutorials/apps/04-sd.md) |

#### Network

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`net_ping`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/net_ping.rs) | get an address and answer `ping` | `net`, feature `net`; Ethernet | [Applications 5](tutorials/apps/05-network.md) |
| [`net_udp`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/net_udp.rs) | UDP echo and a log collector | `net`, feature `net`; Ethernet | [Applications 5](tutorials/apps/05-network.md) |
| [`net_web`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/net_web.rs) | a web page for the board | `net`, `gfx`, feature `net`; Ethernet | [Applications 5](tutorials/apps/05-network.md) |
| [`net_https`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/baremetal/net_https.rs) | the same over HTTPS | `net`, `crypto`, `tls`, `fat`, feature `net`; the test certificate; Ethernet | [Applications 6](tutorials/apps/06-https.md) |

### PYNQ-Z2: RTOS

As [bare metal](#pynq-z2-bare-metal), with `--platform rtos`, `-o work/z2-rtos` and the programs of `apps/base/rtos/`;
what a program needs is [added the same way](#adding-what-a-program-needs).

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`control`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/control.rs) | a 10 ms control loop copying the buttons to the LEDs | — | [RTOS 1](tutorials/rtos/01-control-loop.md) |
| [`rt_lights`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_lights.rs) | a mood light: four tasks, PWM, buttons by interrupt | — | [Applications 1](tutorials/apps/01-base-lights.md) |
| [`rt_irq_latency`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_irq_latency.rs) | how long an interrupt takes to reach a task, as a histogram | — | [Applications 1](tutorials/apps/01-base-lights.md) |
| [`rt_video`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_video.rs) | an animation paced by the display's frame interrupt | `gfx`, `video`; HDMI monitor | [Applications 2](tutorials/apps/02-hdmi.md) |
| [`rt_audio`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_audio.rs) | audio by timer interrupt beside a control loop | `audio`; `chime.wav`; headphones | [Applications 7](tutorials/apps/07-integration.md) |
| [`rt_net`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_net.rs) | a web server beside a 10 ms control loop | `net`, feature `net`; Ethernet | [Applications 5](tutorials/apps/05-network.md) |
| [`rt_https`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_https.rs) | the same over HTTPS | `net`, `crypto`, `tls`, feature `net`; the test certificate; Ethernet | [Applications 6](tutorials/apps/06-https.md) |
| [`rt_station`](https://github.com/jwkunz/Hardroxide/blob/main/apps/base/rtos/rt_station.rs) | **everything at once**: HTTPS dashboard, graphics, audio, SD card, control loop | `audio`, `crypto`, `fat`, `gfx`, `net`, `tls`, `video`, feature `net`; `chime.wav`, the test certificate; HDMI, headphones, Ethernet; a card is optional | [Applications 7](tutorials/apps/07-integration.md) |

### PYNQ-Z2: processor and kernel

These are complete crates of their own in `baremetal/`: no generation step.

```console
$ cd baremetal/rt-smoke
$ cargo build --release
$ hardroxide run target/armv7a-none-eabihf/release/rt-smoke --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 20
```

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`ps7-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/ps7-smoke/src/main.rs) | the processor system: console, clocks, interrupts, exceptions | — | — |
| [`rt-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/rt-smoke/src/main.rs) | the kernel: preemption, queues, priority inheritance, registers | — | — |
| [`rt-jitter`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/rt-jitter/src/main.rs) | the kernel's timing under load, for hours | — | — |
| [`net-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/net-smoke/src/main.rs) | web, HTTPS, TCP and UDP servers, checked by `scripts/net_check.py` | Ethernet | — |

The SD programs of this directory, `sd-info` and `sd-smoke`, build for the Zynq-7000 too and run under emulation; on this
board use the [SD card applications](#sd-card).

### PYNQ-Z2: other designs

Small designs made to exercise one thing each, `fixtures/pynq_z2_<design>.xsa`
([what is in them](reference/fixtures.md)). The [Your own design](tutorials/own-design/index.md) tutorials go through
them. For Linux, generate the crate, copy the program into `examples/` and use `scripts/board/run.sh` as above.

| Program | Design | What it does | Also needs | Walk-through |
|---|---|---|---|---|
| [`gpio_interactive`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/gpio/gpio_interactive.rs) | `gpio` | walk the LEDs and print the inputs | — | — |
| [`timer_irq`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/peripherals/timer_irq.rs) | `peripherals` | wait for a timer's interrupt | overlay | [Own design 1](tutorials/own-design/01-timer-spi-irq.md) |
| [`uio_backend`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/peripherals/uio_backend.rs) | `peripherals` | drive the timer through its UIO device | overlay | — |
| [`async_bench`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/peripherals/async_bench.rs) | `peripherals` | register access cost; async interrupts | overlay; feature `async` of `hardroxide-irq` | — |
| [`lifecycle`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/peripherals/lifecycle.rs) | `peripherals` | loading, dropping and reloading a design | — | — |
| [`uart_pair`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/loopback/uart_pair.rs) | `loopback` | two UARTs wired together | — | [Own design 2](tutorials/own-design/02-uart-iic-loopback.md) |
| [`iic_pair`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/loopback/iic_pair.rs) | `loopback` | two I2C cores on one bus | — | [Own design 2](tutorials/own-design/02-uart-iic-loopback.md) |
| [`uart_irq`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/loopback/uart_irq.rs) | `loopback` | UART interrupts | overlay | [Own design 2](tutorials/own-design/02-uart-iic-loopback.md) |
| [`iic_irq`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/loopback/iic_irq.rs) | `loopback` | an I2C interrupt | overlay | [Own design 2](tutorials/own-design/02-uart-iic-loopback.md) |
| [`vadd_run`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/hls_vadd/vadd_run.rs) | `hls_vadd` | run an HLS kernel | — | [Own design 3](tutorials/own-design/03-hls-vadd.md) |
| [`vadd_irq`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/hls_vadd/vadd_irq.rs) | `hls_vadd` | the same, waiting for its interrupt | overlay | [Own design 3](tutorials/own-design/03-hls-vadd.md) |
| [`scale_run`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/hls_scale/scale_run.rs) | `hls_scale` | an HLS kernel that reads and writes memory | `u-dma-buf` | [Own design 4](tutorials/own-design/04-hls-pointers.md) |
| [`dma_irq`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/dma_loopback/dma_irq.rs) | `dma_loopback` | DMA by interrupt | overlay, `u-dma-buf` | [Own design 5](tutorials/own-design/05-dma.md) |
| [`dma_edge`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/dma_loopback/dma_edge.rs) | `dma_loopback` | DMA sizes, timeouts and speed | `u-dma-buf` | [Own design 5](tutorials/own-design/05-dma.md) |
| [`fifo_edge`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/fifo/fifo_edge.rs) | `fifo` | stream FIFO packet lengths | overlay | [Own design 6](tutorials/own-design/06-stream-fifo.md) |
| [`bram_bytes`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/mem_uart/bram_bytes.rs) | `mem_uart` | block RAM as bytes | — | — |
| [`intc_timer`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests/intc/intc_timer.rs) | `intc` | interrupts through an interrupt controller | overlay | — |

For bare metal and RTOS, use the board-ready copy of the design, `fixtures/board/pynq_z2_<design>.xsa`:

| Program | Design | Flavor | What it does | Walk-through |
|---|---|---|---|---|
| [`leds`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_bm/baremetal_gpio/leds.rs) | `gpio` | bare metal | mirror the inputs onto the LEDs | — |
| [`timer_ticks`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_bm/baremetal_timer/timer_ticks.rs) | `peripherals` | bare metal | a PL timer's interrupt, measured | [Own design 8](tutorials/own-design/08-baremetal-timer.md) |
| [`dma_speed`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_bm/baremetal_dma/dma_speed.rs) | `dma_loopback` | bare metal | DMA speed | [Own design 9](tutorials/own-design/09-baremetal-dma.md) |
| [`control`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_bm/rtos_gpio/control.rs) | `gpio` | RTOS | a 10 ms control loop | — |

## RFSoC 4x2

The base overlay is `fixtures/board/rfsoc4x2_base_overlay.xsa` ([what is in it](reference/base-overlay.md#rfsoc-4x2)).
The programs are in `apps/rfsoc4x2/`; the radio ones share the files of its `common/` folder.

### RFSoC 4x2: Linux

The board started from its PYNQ card ([setup](tutorials/rfsoc/08-linux.md)); wait two minutes after switching on.

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --name design_1 -o work/rf-linux --hardroxide-path .
$ cp apps/rfsoc4x2/linux/lights.rs work/rf-linux/examples/
$ export BOARD=xilinx@192.168.2.99
$ scripts/board/run.sh work/rf-linux lights
```

For the radio, also `cp -r apps/rfsoc4x2/common work/rf-linux/examples/`.

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`lights`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/linux/lights.rs) | self-test, then buttons and switches drive the LEDs by interrupt | — | [RFSoC 8](tutorials/rfsoc/08-linux.md) |
| [`dashboard`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/linux/dashboard.rs) | a web page: temperature, buttons, switches, LEDs, power monitors | — | [RFSoC 9](tutorials/rfsoc/09-linux-web.md) |
| [`pl_memory`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/linux/pl_memory.rs) | the FPGA's own 4 GiB of DDR4 | clock chips programmed (the image does it) | [RFSoC 10](tutorials/rfsoc/10-linux-memory.md) |
| [`radio_loopback`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/linux/radio_loopback.rs) | **the radio**: a tone out of each DAC, found by an ADC | `common/`; `u-dma-buf`; SMA cables | [RFSoC 11](tutorials/rfsoc/11-radio.md) |
| [`tour`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/base/tour.rs) | a self-check of the overlay's processor-side blocks | — | — |

### RFSoC 4x2: bare metal

The board in JTAG boot mode ([setup](tutorials/rfsoc/index.md#board-setup)):

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --platform baremetal --name design_1 -o work/rf-bm --hardroxide-path .
$ cp apps/rfsoc4x2/baremetal/lights.rs work/rf-bm/examples/
$ cd work/rf-bm
$ cargo build --release --example lights
$ hardroxide run target/aarch64-unknown-none/release/examples/lights --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 70
```

<a id="adding-what-an-rfsoc-program-needs"></a>For a radio program, before building, inside the crate:
`cp -r ../../apps/rfsoc4x2/common examples/`, `cp ../../fixtures/rfsoc4x2/clocks/*.txt examples/`, and
`cargo add --dev --path ../../crates/hardroxide-board`.

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`lights`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/baremetal/lights.rs) | self-test, then buttons and switches drive the LEDs by interrupt | — | [RFSoC 2](tutorials/rfsoc/02-base-overlay.md) |
| [`radio_loopback`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/baremetal/radio_loopback.rs) | **the radio**: a tone out of each DAC, found by an ADC | `board`; `common/` and the clock register files; SMA cables | [RFSoC 11](tutorials/rfsoc/11-radio.md) |
| [`tour`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/base/tour.rs) | a self-check of the overlay's processor-side blocks | — | — |

### RFSoC 4x2: RTOS

As [bare metal](#rfsoc-4x2-bare-metal), with `--platform rtos`, `-o work/rf-rtos` and the programs of
`apps/rfsoc4x2/rtos/`. For `rt_dashboard`, add inside the crate:
`cargo add --dev --path ../../crates/hardroxide-net` and
`cargo add --dev --path ../../crates/hardroxide-psu --no-default-features --features net`.

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`rt_lights`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/rtos/rt_lights.rs) | a mood light: several tasks, interrupts that wake them | — | [RFSoC 4](tutorials/rfsoc/04-mood-light.md) |
| [`rt_dashboard`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/rtos/rt_dashboard.rs) | a web page beside a 10 ms control loop | `net`; `psu` with feature `net`; Ethernet | [RFSoC 7](tutorials/rfsoc/07-network-sd.md) |
| [`rt_radio_loopback`](https://github.com/jwkunz/Hardroxide/blob/main/apps/rfsoc4x2/rtos/rt_radio_loopback.rs) | **the radio** beside a 1 ms control loop | `board`; `common/` and the clock register files; SMA cables | [RFSoC 11](tutorials/rfsoc/11-radio.md) |

### RFSoC 4x2: processor and kernel

Complete crates of their own in `baremetal/`: no generation step.

```console
$ cd baremetal/psu-smoke
$ cargo build --release --target aarch64-unknown-none
$ hardroxide run target/aarch64-unknown-none/release/psu-smoke --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 20
```

#### Bare metal programs

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`psu-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/psu-smoke/src/main.rs) | the processor system: clocks, caches, memory, timers | — | [RFSoC 1](tutorials/rfsoc/01-first-light.md) |
| [`rfsoc-board`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/rfsoc-board/src/main.rs) | the board's own chips: identity, power monitors, display, clock chips | — | [RFSoC 3](tutorials/rfsoc/03-board-hardware.md) |
| [`i2c-info`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/i2c-info/src/main.rs) | read the I2C buses and power monitors; writes no settings | — | — |
| [`net-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/net-smoke/src/main.rs) | web, HTTPS, TCP and UDP servers | Ethernet | [RFSoC 7](tutorials/rfsoc/07-network-sd.md) |
| [`sd-info`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/sd-info/src/main.rs) | identify the SD card and list it; cannot write | a card | [RFSoC 7](tutorials/rfsoc/07-network-sd.md) |
| [`sd-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/sd-smoke/src/main.rs) | **erases the named card** and tests the file system | `HRX_ERASE_SERIAL` | [RFSoC 7](tutorials/rfsoc/07-network-sd.md) |

#### Kernel programs

| Program | What it does | Also needs | Walk-through |
|---|---|---|---|
| [`rt-smoke`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/rt-smoke/src/main.rs) | the kernel: preemption, queues, priority inheritance, registers | — | [RFSoC 6](tutorials/rfsoc/06-kernel-timing.md) |
| [`rt-jitter`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/rt-jitter/src/main.rs) | the kernel's timing under load, for hours | — | [RFSoC 6](tutorials/rfsoc/06-kernel-timing.md) |
| [`rt-board`](https://github.com/jwkunz/Hardroxide/blob/main/baremetal/rt-board/src/main.rs) | the board's chips, each looked after by a task | — | [RFSoC 5](tutorials/rfsoc/05-kernel-board.md) |

### RFSoC 4x2: other designs

One test program for each design in `fixtures/rfsoc4x2/rfsoc4x2_<design>.xsa`
([what is in them](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/rfsoc4x2/README.md)), for Linux and for bare
metal. `scripts/test-board-rfsoc.sh` generates and runs the bare-metal ones; for Linux see
[the guide](guide/linux-ultrascale.md).

| Design | What it checks | Linux program | Linux also needs | Bare-metal program |
|---|---|---|---|---|
| `gpio` | LEDs, inputs and the inputs' interrupt | [`Linux`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/gpio/gpio_check.rs) | — | [`bare metal`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/gpio/gpio_check.rs) |
| `intc` | two sources behind an interrupt controller | [`Linux`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/intc/controller.rs) | overlay | [`bare metal`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/intc/controller.rs) |
| `loopback` | every peripheral against its partner inside the FPGA | [`Linux`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/loopback/drivers.rs) | overlay | [`bare metal`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/loopback/drivers.rs) |
| `dma` | two DMAs by interrupt, one with 40-bit addresses | [`Linux`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/dma/both.rs) | overlay, `u-dma-buf` | [`bare metal`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/dma/both.rs) |
| `hls_vadd` | an HLS kernel and its interrupt | [`Linux`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/hls_vadd/run.rs) | overlay | [`bare metal`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/hls_vadd/run.rs) |
| `hls_scale` | an HLS kernel that reads and writes memory | [`Linux`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc_linux/hls_scale/run.rs) | `u-dma-buf` | [`bare metal`](https://github.com/jwkunz/Hardroxide/blob/main/fixtures/board_tests_rfsoc/hls_scale/run.rs) |
