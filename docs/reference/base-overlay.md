# PYNQ base overlay (board-ready)

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

## What the processor reaches

| Instance (field of `Design`) | What it is | Driver today |
|---|---|---|
| `btns_gpio`, `switches_gpio`, `leds_gpio`, `rgbleds_gpio` | the four buttons, two switches, four LEDs, two RGB LEDs (six pins) | `AxiGpio` |
| `system_interrupts` | AXI interrupt controller; its `irq` output is GIC interrupt 61 (SPI 29) | `AxiIntc` |
| `video_axi_vdma`, `..._vtc_out`, `..._axi_dynclk`, `..._color_convert`, `..._pixel_unpack`, and the `video_hdmi_in_*` set | the HDMI input and output pipelines | raw windows and generated HLS wrappers (typed drivers planned for [2.2](../decisions/0004-base-overlay-applications.md)) |
| `audio_codec_ctrl_0` | the audio codec's I2S controller (AXI-Lite only; no stream port, no interrupt) | a raw window (planned for 2.3) |
| `trace_analyzer_*`, `iop_*_mb_bram_ctrl` | the logic analyser and the MicroBlaze program memories | `AxiDma`, `AxiBram`, HLS wrappers |

Interrupt inputs of `system_interrupts`: the buttons' GPIO is input 13, the switches' 14, the LEDs' 8, the HDMI frontends'
GPIO 4 and 5, the video timing controllers 2 and 3, the VDMA 0 and 1, and the trace DMAs 6 and 7. The inputs are level
sensitive. The PS side (all from the PYNQ-Z2 preset): UART 0 on MIO 14/15 (the console), Ethernet 0 on MIO 16 to 27, SD 0 on
MIO 40 to 45, USB 0, QSPI.

## Applications

The programs in `apps/base/` build against this crate (`cargo test -p hardroxide-gen --test apps`) and are described in
[Base overlay 1](../tutorials/apps/01-base-lights.md):

| Program | Flavor | What it does |
|---|---|---|
| `selftest` | bare metal | an automated check of the clocks, LEDs, RGB LEDs and the interrupt path (controller to GIC to processor) |
| `lights` | bare metal | buttons through the interrupt controller: count on the LEDs, colours from the switches |
| `rt_lights` | RTOS | a mood light: four tasks with PWM, rainbow patterns, brightness and on/off from the buttons |
| `rt_irq_latency` | RTOS | interrupt-to-task latency under load, as a histogram |

`scripts/test-board.sh` runs the self-checking ones (and the earlier tutorials' programs) on a board in one go.
