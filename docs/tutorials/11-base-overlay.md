# Tutorial 11: A design you did not build

**Design:** `fixtures/real/pynq_base_overlay.xsa` · **Time:** 15 minutes ·
**Needs:** [setup](index.md#before-you-start-once)

Everything so far used small designs made for testing. This tutorial uses the
**PYNQ base overlay**: the large design that ships with PYNQ for this board, built by
someone else, with video input and output, audio, trace analyzers, LEDs, buttons and
switches, and four MicroBlaze processors that drive the Arduino, Pmod and Raspberry
Pi connectors. It is the realistic test of whether Hardroxide copes with a design you
did not write. (`fixtures/real/` explains where it comes from.)

## 1. Inspect it

```console
$ hardroxide inspect fixtures/real/pynq_base_overlay.xsa
```

The output is long, so one very long line is shortened here:

```text
base (zynq 7z020, Vivado 2024.1)
interface hash: c52ebef9...
FCLK0: 100000000 Hz
FCLK1: 142857132 Hz
FCLK2: 200000000 Hz
FCLK3: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x40000000      64 KiB  iop_pmoda_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x41200000      64 KiB  switches_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 14 of system_interrupts
0x41210000      64 KiB  btns_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 13 of system_interrupts
0x41220000      64 KiB  video_hdmi_in_frontend_axi_gpio_hdmiin  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 4 of system_interrupts
0x41230000      64 KiB  video_hdmi_out_frontend_hdmi_out_hpd_video  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 5 of system_interrupts
0x41240000      64 KiB  rgbleds_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
0x41250000      64 KiB  leds_gpio  ->  hardroxide_ip::gpio::AxiGpio  (xilinx.com:ip:axi_gpio:2.0)
                           irq ip2intc_irpt -> input 8 of system_interrupts
0x41800000      64 KiB  system_interrupts  ->  hardroxide_ip::intc::AxiIntc  (xilinx.com:ip:axi_intc:4.1)
                           irq irq -> IRQ_F2P[0] (GIC SPI 29)
0x42000000      64 KiB  iop_pmodb_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x43000000      64 KiB  video_axi_vdma  ->  raw register window  (xilinx.com:ip:axi_vdma:6.3)
                           irq mm2s_introut -> input 1 of system_interrupts
                           irq s2mm_introut -> input 0 of system_interrupts
0x43c00000      64 KiB  audio_codec_ctrl_0  ->  raw register window  (xilinx.com:user:audio_codec_ctrl:1.0)
0x43c10000      64 KiB  video_hdmi_out_frontend_axi_dynclk  ->  raw register window  (digilentinc.com:ip:axi_dynclk:1.0)
0x43c20000      64 KiB  video_hdmi_out_frontend_vtc_out  ->  raw register window  (xilinx.com:ip:v_tc:6.2)
                           irq irq -> input 2 of system_interrupts
0x43c30000      64 KiB  video_hdmi_in_frontend_vtc_in  ->  raw register window  (xilinx.com:ip:v_tc:6.2)
                           irq irq -> input 3 of system_interrupts
0x43c40000      64 KiB  video_hdmi_in_pixel_pack  ->  crate::ips::VideoHdmiInPixelPack (HLS)  (xilinx.com:hls:pixel_pack:1.0)
0x43c50000      64 KiB  video_hdmi_in_color_convert  ->  crate::ips::VideoHdmiInColorConvert (HLS)  (xilinx.com:hls:color_convert:1.0)
0x43c60000      64 KiB  video_hdmi_out_color_convert  ->  crate::ips::VideoHdmiOutColorConvert (HLS)  (xilinx.com:hls:color_convert:1.0)
0x43c70000      64 KiB  video_hdmi_out_pixel_unpack  ->  crate::ips::VideoHdmiOutPixelUnpack (HLS)  (xilinx.com:hls:pixel_unpack:1.0)
0x44000000      64 KiB  iop_arduino_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x46000000      64 KiB  iop_rpi_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x80420000      64 KiB  trace_analyzer_pmodb_axi_dma_0  ->  hardroxide_ip::dma::AxiDma  (xilinx.com:ip:axi_dma:7.1)
                           irq s2mm_introut -> input 6 of system_interrupts
0x80430000      64 KiB  trace_analyzer_pi_axi_dma_0  ->  hardroxide_ip::dma::AxiDma  (xilinx.com:ip:axi_dma:7.1)
                           irq s2mm_introut -> input 7 of system_interrupts
0x83c10000      64 KiB  trace_analyzer_pi_trace_cntrl_64_0  ->  crate::ips::TraceAnalyzerPiTraceCntrl640 (HLS)  (xilinx.com:hls:trace_cntrl_64:1.4)
0x83c20000      64 KiB  trace_analyzer_pmodb_trace_cntrl_32_0  ->  crate::ips::TraceAnalyzerPmodbTraceCntrl320 (HLS)  (xilinx.com:hls:trace_cntrl_32:1.4)

24 instances: 13 with a curated driver, 6 HLS wrapper(s), 5 raw register window(s)
45 instance(s) are not reachable from the PS (private to another master such as a MicroBlaze) and get no field: address_remap_0, iop_arduino_arduino_gpio, iop_arduino_iic ...
```

Three things to notice:

- **24 instances are reachable from the processor**, and they are classified:
  13 get a curated driver (`AxiGpio`, `AxiBram`, `AxiDma`, `AxiIntc`), 6 get a
  generated HLS wrapper (the video blocks are HLS IP), 5 are raw register windows
  (for example `video_axi_vdma`, which Hardroxide has no driver for).
- Every interrupt is traced to its input of `system_interrupts`, an interrupt
  controller, and from there to `IRQ_F2P[0]`.
- **45 other instances are *not* reachable**: they belong to the four MicroBlaze
  subsystems. Each MicroBlaze has its own private address map in which *its* timer,
  UART and SPI live, and those maps overlap each other and the processor's. Only the
  processor's own map says what software on the Zynq can reach, so only those
  instances get a field.

## 2. Generate, and look at the examples

```console
$ hardroxide generate fixtures/real/pynq_base_overlay.xsa -o work/base --hardroxide-path .
$ ls work/base/examples
board_check.rs  bram_test.rs  dma_loopback.rs  gpio_smoke.rs  hls_smoke.rs  irq_probe.rs
```

There is no `timer_delay` or `spi_loopback`: those cores exist, but only inside the
MicroBlaze subsystems, so the processor cannot use them.

With several GPIOs and several BRAMs in the design, the examples take the instance
as an argument, and `gpio_smoke` defaults to an instance that has output pins
(the LEDs):

```console
$ grep -n "Several instances\|// Available" work/base/examples/gpio_smoke.rs
11:    // Several instances: pass the name as the first argument. Default: leds_gpio.
12:    // Available: leds_gpio, rgbleds_gpio, video_hdmi_in_frontend_axi_gpio_hdmiin, video_hdmi_out_frontend_hdmi_out_hpd_video, btns_gpio, switches_gpio
```

## 3. Run it

Free the PL ([setup](index.md#before-you-start-once)), then:

```console
$ scripts/board/run.sh work/base board_check
1. loading bitstream and applying clocks ...
   ok
2. FCLK readback (SLCR) vs. the design:
   FCLK0: 100000000 Hz (want 100000000) ok
   FCLK1: 142857142 Hz (want 142857132) ok
   FCLK2: 200000000 Hz (want 200000000) ok
   FCLK3: 100000000 Hz (want 100000000) ok
3. interrupt UIO devices (needs interrupts.dtso applied):
   system_interrupts.irq: no UIO device for interrupt `system_interrupts.irq` (device-tree node `system_interrupts_irq`, base 0x41800000); is the generated overlay (interrupts.dtso) applied?
1 check(s) failed
```

The design programmed, and all four PL clocks (100, 142.857, 200 and 100 MHz) read
back correctly. (Step 3, which opens interrupt devices, needs the overlay and is not
shown.)

```console
$ scripts/board/run.sh work/base gpio_smoke            # walks the 4 LEDs (the default)
$ scripts/board/run.sh work/base gpio_smoke btns_gpio  # an input-only GPIO: reads it
channel 1: 0x0
$ scripts/board/run.sh work/base bram_test iop_rpi_mb_bram_ctrl
65536 bytes: 0 mismatching words
```

That last one writes and reads back the 64 KiB program memory of the Raspberry Pi
subsystem's MicroBlaze, through the processor's window onto it: the way the
processor loads a MicroBlaze program.

## 4. A program of your own

```rust,ignore
{{#include ../../fixtures/board_tests/base/base_tour.rs}}
```

```console
$ cp fixtures/board_tests/base/base_tour.rs work/base/examples/
$ scripts/board/run.sh work/base base_tour
loaded the PYNQ base overlay through a generated crate
switches (2 bits): 0x0
buttons  (4 bits): 0x0
system_interrupts: enabled 0x0, pending 0x0
```

It reads the switches and buttons, walks the LEDs, and reads the system interrupt
controller, all through typed drivers generated from a design you never saw the
source of.

## Putting the board back

`prep.sh` stopped the display and other services so the PL could be reprogrammed.
**Reboot the board** to get the stock desktop back.

## What you did

You pointed Hardroxide at a large third-party design and got a typed, compiling,
working crate in one command, with the design's structure (which blocks the
processor can reach, which interrupts go where) explained by `inspect` before you
generated anything.

That is the end of the tutorials. To use Hardroxide on your own design, start with
[Exporting an XSA from Vivado](../guide/vivado.md) and follow
[The workflow](../guide/workflow.md).
