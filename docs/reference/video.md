# hardroxide-video

HDMI output for the PYNQ base overlay. The overlay's pipeline is: frame buffers in memory, an AXI VDMA read channel
([`AxiVdma`](ip-vdma.md)), a pixel unpacker and a colour converter (HLS cores), the HDMI transmitter, a video timing generator
([`VideoTimingController`](ip-vtc.md)) pacing the stream and a reconfigurable pixel clock ([`AxiDynclk`](ip-dynclk.md)).
`Display` brings it up for a `VideoMode`; the graphics library [`hardroxide-gfx`](gfx.md) draws on the buffers. `no_std`. Pixels are 32
bits, `0x00RRGGBB`, four bytes in memory.

> **Verified on a PYNQ-Z2 (2026-11-21)** at 800x600, 1024x768, 1280x720, 1280x1024 and 1920x1080, at 60 frames per second with double buffering; **640x480 is rejected by the
> tested monitor** (open item). The steps follow PYNQ's video pipeline (Apache-2.0) for the colour and pixel-format settings; the timing controller and DMA sequences were
> corrected against the Linux driver's register state read over JTAG. See [Verification gaps](../08-verification-gaps.md).

```rust,ignore
pub const BYTES_PER_PIXEL: u32 = 4;
pub struct VideoMode
pub const fn bytes_per_line(&self) -> u32
pub const fn frame_bytes(&self) -> u32
pub const fn frame_pixels(&self) -> usize
pub const VGA_640X480_60: VideoMode
pub const SVGA_800X600_60: VideoMode
pub const XGA_1024X768_60: VideoMode
pub const HD_1280X720_60: VideoMode
pub const SXGA_1280X1024_60: VideoMode
pub const FHD_1920X1080_60: VideoMode
pub const MODES: [VideoMode; 6]
pub fn mode_for(width: u32, height: u32) -> Option<&'static VideoMode>
pub enum DisplayError
pub struct Display<'a, R: Mmio>
pub fn set_colour(&self, m: &colour::Matrix) -> Result<(), MmioError>
pub fn start(&self, mode: &VideoMode, frames: &[u32]) -> Result<u32, DisplayError>
pub fn stop(&self) -> Result<(), MmioError>
pub fn show(&self, n: u32) -> Result<(), MmioError>
pub fn showing(&self) -> Result<u32, MmioError>
pub fn wait_frame(&self, timeout: Duration) -> Result<bool, MmioError>
pub fn check(&self) -> Result<(), VdmaError>
pub fn as_pixels(bytes: &mut [u8]) -> Option<&mut [u32]>
```

`Display` borrows the five pieces (fields `dynclk` with `ref_clk_hz` (the clock on the generator's reference input, 100 MHz on the base overlay), `vtc`, `vdma`, and the raw `colour` and `unpack` register windows, for example
`d.video_hdmi_out_color_convert.regs()`). `start(mode, frames)`: stops the timing generator, programs the pixel clock, loads the identity
colour matrix and 32-bit pixel mode, starts the DMA on the frame buffers (their 32-bit physical addresses, each `mode.frame_bytes()` long) and starts
the timing generator last, which releases the stream. It returns the pixel clock produced. `show(n)` switches to buffer `n` at the next frame.

Modes (all 60 Hz): 640x480 (25.2 MHz; not working on the tested monitor), 800x600 (40), 1024x768 (65), 1280x720 (74.25), 1280x1024 (108), 1920x1080 (148.5); the timings
are the VESA and CEA-861 ones and are checked for consistency in the tests. `colour::IDENTITY` passes pixels through (byte 0 blue, so `0x00RRGGBB`
little-endian); `colour::SWAP_RB` swaps red and blue. Frame buffers in the non-cacheable `.dma` section need no cache maintenance
([`static_dma_buffer!`](dma.md)); `as_pixels` views such a buffer's bytes as pixels. 1920x1080 works.

**Frame buffer placement.** The base overlay's video DMA reaches DDR through one HP port, and the design maps only **`0x1000_0000` to `0x1FFF_FFFF`** to it: a frame
buffer anywhere else (the lower half of DDR, where the program and `.dma` live) ends in a DMA decode error (status bit 6) and a blank screen. The applications put the frame
buffers at `0x1000_0000` and make that window non-cacheable with [`ps7::mem::set_non_cacheable`](ps7.md#memory-mmu-caches-l2-mem) so the processor's drawing and the DMA's
reads agree. `0x1000_0000` + 4 MiB per 720p frame (3.5 MiB rounded up to whole sections), 8 MiB per 1080p frame.
