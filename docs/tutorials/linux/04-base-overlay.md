# Linux 4: The whole overlay

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 15 minutes · **Needs:** [Linux 1](01-lights.md)

You have used the lights, the interrupt controller and a block RAM. The **PYNQ base overlay** holds much more: video
input and output, audio, trace analyzers, and four MicroBlaze processors that drive the Arduino, Pmod and Raspberry Pi
connectors. Someone else designed it, in Vivado, years before you opened it, which makes it a fair test of what you get
for a design you did not write.

## 1. Everything the processor can reach

```console
$ hardroxide inspect fixtures/board/pynq_base_overlay.xsa
```

The output is long, so its very long last line is shortened here:

```text
base (zynq 7z020, Vivado 2024.1)
interface hash: c52ebef9797f3034499e0948a5b9e93ca326eeb6367a4d69b5630111b5195f09
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
0x43000000      64 KiB  video_axi_vdma  ->  hardroxide_ip::vdma::AxiVdma  (xilinx.com:ip:axi_vdma:6.3)
                           irq mm2s_introut -> input 1 of system_interrupts
                           irq s2mm_introut -> input 0 of system_interrupts
0x43c00000      64 KiB  audio_codec_ctrl_0  ->  hardroxide_ip::audio::AudioCodecCtrl  (xilinx.com:user:audio_codec_ctrl:1.0)
0x43c10000      64 KiB  video_hdmi_out_frontend_axi_dynclk  ->  hardroxide_ip::dynclk::AxiDynclk  (digilentinc.com:ip:axi_dynclk:1.0)
0x43c20000      64 KiB  video_hdmi_out_frontend_vtc_out  ->  hardroxide_ip::vtc::VideoTimingController  (xilinx.com:ip:v_tc:6.2)
                           irq irq -> input 2 of system_interrupts
0x43c30000      64 KiB  video_hdmi_in_frontend_vtc_in  ->  hardroxide_ip::vtc::VideoTimingController  (xilinx.com:ip:v_tc:6.2)
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

24 instances: 18 with a curated driver, 6 HLS wrapper(s), 0 raw register window(s)
45 instance(s) are not reachable from the PS (private to another master such as a MicroBlaze) and get no field: address_remap_0, iop_arduino_arduino_gpio, iop_arduino_iic ...
```

Three things to notice:

- **24 instances are reachable from the processor**, and they are classified: 18 get a curated driver (`AxiGpio`,
  `AxiBram`, `AxiDma`, `AxiIntc`, and the video and audio blocks), 6 get a generated wrapper because they were written
  in HLS, a C-to-hardware tool.
- Every interrupt is traced to its input of `system_interrupts` and from there to `IRQ_F2P[0]`.
- **45 other instances are *not* reachable**: they belong to the four MicroBlaze subsystems. Each MicroBlaze has its
  own private address map in which *its* timer, UART and SPI live. Only the processor's own map says what software on
  the Zynq can reach, so only those instances get a field.

## 2. The examples, by instance

With several blocks of one kind, the generated examples take the instance's name:

```console
$ grep -n "Several instances\|// Available" work/base/examples/gpio_smoke.rs
11:    // Several instances: pass the name as the first argument. Default: leds_gpio.
12:    // Available: leds_gpio, rgbleds_gpio, video_hdmi_in_frontend_axi_gpio_hdmiin, video_hdmi_out_frontend_hdmi_out_hpd_video, btns_gpio, switches_gpio
$ scripts/board/run.sh work/base gpio_smoke                 # walks the 4 LEDs (the default)
$ scripts/board/run.sh work/base gpio_smoke btns_gpio       # an input-only GPIO: reads it
```

There is no `timer_delay` or `spi_loopback`: those cores exist, but only inside the MicroBlaze subsystems.

> **Don't run `dma_loopback` here.** It is for designs whose DMA output is wired back to its input; this design's DMA
> engines are wired to the trace analyzers.

## 3. Where to go next

- **Video, sound, the SD card and the network** of this overlay: the [application tutorials](../apps/index.md), which
  run without an operating system and on the real-time kernel.
- **DMA, HLS kernels, timers, serial buses:** [Your own design](../own-design/index.md).
- **A design from your Vivado project:** [Exporting an XSA](../../guide/vivado.md) and
  [The workflow](../../guide/workflow.md).

## Putting the board back

`prep.sh` stopped the display and other services so the FPGA could be reprogrammed. To get them back, shut the board
down, then switch it off and on.
