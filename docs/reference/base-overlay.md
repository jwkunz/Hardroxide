# PYNQ base overlay (board-ready)

The examples of each board run on its base overlay, the FPGA design PYNQ ships: the [PYNQ-Z2's](#pynq-z2) below, and
the [RFSoC 4x2's](#rfsoc-4x2).

## PYNQ-Z2

`fixtures/board/pynq_base_overlay.xsa` is the PYNQ **base overlay** (PYNQ project, Apache-2.0) in a form
`hardroxide run` can start: the overlay's own bitstream and hardware handoff, plus a `ps7_init` for the PYNQ-Z2. The
XSA that PYNQ ships has no `ps7_init` (it is the `.bit` and `.hwh` repackaged, `fixtures/real/pynq_base_overlay.xsa`),
and applying the overlay's own DDR settings through Vivado produced a DDR that failed a memory test on the board, so the
`ps7_init` is built from the board preset used by the other board fixtures ([fixtures](fixtures.md)) plus the base overlay's
PS-to-PL settings:

| Step | File |
|---|---|
| a PS-only design with the board preset and the base overlay's PS-to-PL settings (clocks 100 / 142.857 / 200 / 100 MHz, both master GPs, a slave GP, HP0 and HP2, fabric interrupts, EMIO GPIO and I2C 1) | `fixtures/scripts/build_pynq_base_ps7.tcl` (Vivado 2025.2) |
| add the `ps7_init.{tcl,c,h}` it generates to the overlay's `.bit` and `.hwh` | `fixtures/scripts/package_base_overlay.py` |

`hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal|rtos` then gives a crate for the whole overlay
(a field for each instance the processor can reach; the MicroBlaze subsystems' private peripherals get none). On the board:

```console
$ hardroxide run target/armv7a-none-eabihf/release/examples/board_check --xsa fixtures/board/pynq_base_overlay.xsa
== board check (base) ==
PL configured, interface c52ebef9797f3034499e0948a5b9e93ca326eeb6367a4d69b5630111b5195f09
FCLK0: 100000000 Hz (want 100000000) ok
FCLK1: 142857142 Hz (want 142857132) ok
FCLK2: 200000000 Hz (want 200000000) ok
FCLK3: 100000000 Hz (want 100000000) ok
irq system_interrupts.irq -> GIC id 61 (level)
all checks passed; next run the per-IP examples
```

## What the processor reaches (PYNQ-Z2)

| Instance (field of `Design`) | What it is | Driver today |
|---|---|---|
| `btns_gpio`, `switches_gpio`, `leds_gpio`, `rgbleds_gpio` | the four buttons, two switches, four LEDs, two RGB LEDs (six pins) | `AxiGpio` |
| `system_interrupts` | AXI interrupt controller; its `irq` output is GIC interrupt 61 (SPI 29) | `AxiIntc` |
| `video_axi_vdma`, `..._vtc_out`, `..._axi_dynclk`, `..._color_convert`, `..._pixel_unpack`, and the `video_hdmi_in_*` set | the HDMI input and output pipelines | `AxiVdma`, `VideoTimingController`, `AxiDynclk` (2.2) and generated HLS wrappers; [`hardroxide-video`](video.md) drives the output |
| `audio_codec_ctrl_0` | the audio codec's I2S controller (AXI-Lite only; no stream port, no interrupt) | a raw window (planned for 2.3) |
| `trace_analyzer_*`, `iop_*_mb_bram_ctrl` | the logic analyser and the MicroBlaze program memories | `AxiDma`, `AxiBram`, HLS wrappers |

Interrupt inputs of `system_interrupts`: the buttons' GPIO is input 13, the switches' 14, the LEDs' 8, the HDMI frontends'
GPIO 4 and 5, the video timing controllers 2 and 3, the VDMA 0 and 1, and the trace DMAs 6 and 7. The inputs are level
sensitive. The PS side (all from the PYNQ-Z2 preset): UART 0 on MIO 14/15 (the console), Ethernet 0 on MIO 16 to 27, SD 0 on
MIO 40 to 45, USB 0, QSPI.

## Applications (PYNQ-Z2)

The programs in `apps/base/` build against this crate (`cargo test -p hardroxide-gen --test apps`) and are described in
[Base overlay 1](../tutorials/apps/01-base-lights.md):

| Program | Flavor | What it does |
|---|---|---|
| `leds` | bare metal | the LEDs follow the buttons, the switches choose the colour ([Bare metal 1](../tutorials/baremetal/01-jtag.md)) |
| `button_irq` | bare metal | wait for a button through the interrupt controller ([Bare metal 2](../tutorials/baremetal/02-interrupt.md)) |
| `control` | RTOS | a 10 ms control loop copying the buttons to the LEDs ([RTOS 1](../tutorials/rtos/01-control-loop.md)) |
| `lights`, `interrupts`, `block_ram` | Linux | LEDs and switches; a button interrupt through UIO; block RAM ([Linux tutorials](../tutorials/linux/index.md)) |
| `selftest` | bare metal | an automated check of the clocks, LEDs, RGB LEDs and the interrupt path (controller to GIC to processor) |
| `lights` (in `baremetal/`) | bare metal | buttons through the interrupt controller: count on the LEDs, colours from the switches |
| `rt_lights` | RTOS | a mood light: four tasks with PWM, rainbow patterns, brightness and on/off from the buttons |
| `rt_irq_latency` | RTOS | interrupt-to-task latency under load, as a histogram |
| `video_demo` | bare metal | HDMI at 1280x720: test pattern and a double-buffered animation ([Applications 2](../tutorials/apps/02-hdmi.md); not yet run on a board) |
| `rt_video` | RTOS | a 60 fps animation paced by the display's frame interrupt (not yet run on a board) |

`scripts/test-board.sh` runs the self-checking ones (and the earlier tutorials' programs) on a board in one go.

## RFSoC 4x2

`fixtures/board/rfsoc4x2_base_overlay.xsa` is RFSoC-PYNQ's base overlay (BSD 3-Clause; `base.bit` and `base.hwh` taken
unchanged from the 4.0.0 SD card image) with the `psu_init` that running without Linux needs, generated for the board's
preset plus the overlay's processor-to-FPGA settings ([fixtures](fixtures.md#rfsoc-4x2-base-overlay)). `hardroxide run`
starts it, and `hardroxide generate --platform baremetal|rtos` gives a crate for the whole overlay
([bare-metal crate](baremetal.md#zynq-ultrascale)); without `--platform`, one for Linux
([guide](../guide/linux-ultrascale.md)). Its applications are in `apps/rfsoc4x2/`:

| Application | Platform | What it does |
|---|---|---|
| `lights` | bare metal | self test, then buttons and switches drive the LEDs through interrupts ([RFSoC 2](../tutorials/rfsoc/02-base-overlay.md)) |
| `rt_lights` | RTOS | a mood light: several tasks on the kernel ([RFSoC 4](../tutorials/rfsoc/04-mood-light.md)) |
| `rt_dashboard` | RTOS | a web page served beside a 10 ms control loop ([RFSoC 7](../tutorials/rfsoc/07-network-sd.md#a-web-page-on-the-kernel)) |
| `lights` | Linux | the first one as a Linux program ([RFSoC 8](../tutorials/rfsoc/08-linux.md)) |
| `dashboard` | Linux | a web page: temperature, buttons, switches, LEDs, power monitors ([RFSoC 9](../tutorials/rfsoc/09-linux-web.md)) |
| `pl_memory` | Linux | the FPGA's 4 GiB of DDR4: address lines, patterns, speed ([RFSoC 10](../tutorials/rfsoc/10-linux-memory.md)) |

| Block | Address | What it is | Interrupt controller input |
|---|---|---|---|
| `axi_intc_0` | `0x8002_0000` | interrupt controller; output on `pl_ps_irq0[0]` (interrupt 121) | |
| `btns_gpio` | `0x8003_0000` | buttons 0 to 3 (bits 0..3) and the user reset button (bit 4, reads 1 until pressed) | 1 |
| `leds_gpio` | `0x8004_0000` | four white LEDs | |
| `rgbleds_gpio` | `0x8005_0000` | two colour LEDs, each blue, green, red (bits 0..2 and 3..5) | |
| `sws_gpio` | `0x8008_0000` | four switches | 2 |
| `system_management_wiz_0` | `0xB012_0000` | FPGA temperature and voltages (temperature at offset `0x400`) | 0 |
| `iop_pmod0`, `iop_pmod1` | `0x8000_0000`, `0x8001_0000` | the two Pmod MicroBlaze processors' program memories | 3, 4 |
| `radio_rfdc` | `0xB00C_0000` | the RF data converters: DAC tiles 0 and 2, ADC tiles 0 and 2 | |
| `radio_receiver_*`, `radio_transmitter_*` | `0xB000_0000`..`0xB011_FFFF` | capture (packet generators and DMAs) and amplitude control; **in the converters' clock domains** | 5 (the DMAs) |
| `CMAC_*` | `0xA000_0000`, `0xA001_0000` | the 100 Gbit/s Ethernet MAC and its DMA | 6 |
| `ddr4_0` | `0x10_0000_0000` | the FPGA's DDR4: **4 GiB**, which repeats through the 8 GiB the design maps | |

> **Warning.** The blocks in the converters' clock domains, and `ddr4_0` (its reference clock is the clock chip's), do
> not answer until the clock chips are programmed ([`hardroxide-board`](board.md); under PYNQ, `/boot/boot.py` does it
> while starting). An access before that can stall the processor until the next reset.

**Checked on a board** (`fixtures/rfsoc4x2/scripts/probe.py base`, raw register accesses): the LEDs, buttons and
switches; the interrupt path from both GPIOs to the processor; the temperature; both Pmod memories; after programming
the clock chips, all four converter tiles power up and lock and the eight capture DMAs reset; the FPGA's DDR4 at its
start and just below 4 GiB. **Not checked:** data through the radio blocks, the 100 Gbit/s port, the Pmod processors
running, SYZYGY.

