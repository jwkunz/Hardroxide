# AXI VDMA

`hardroxide_ip::vdma` — `xilinx.com:ip:axi_vdma` (PG020), register direct mode, **read channel** (memory to video stream: the display
side). The register layout follows the vendor driver's header. The write channel (video capture) is not wrapped yet.

> **Not yet run on a board** (see [Verification gaps](../08-verification-gaps.md)); the register sequence is unit-tested.

```rust,ignore
pub const MAX_FRAMES: usize = 16;
pub enum VdmaError
pub struct Geometry
pub struct AxiVdma<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn version(&self) -> Result<u32, MmioError>
pub fn reset_read(&self) -> Result<(), VdmaError>
pub fn start_read(&self, frames: &[u32], g: &Geometry) -> Result<(), VdmaError>
pub fn stop_read(&self) -> Result<(), MmioError>
pub fn read_halted(&self) -> Result<bool, MmioError>
pub fn check_read(&self) -> Result<(), VdmaError>
pub fn show_frame(&self, n: u32) -> Result<(), MmioError>
pub fn current_frame(&self) -> Result<u32, MmioError>
pub fn enable_frame_interrupt(&self) -> Result<(), MmioError>
pub fn disable_interrupts(&self) -> Result<(), MmioError>
pub fn frame_done(&self) -> Result<bool, MmioError>
pub fn ack_frame(&self) -> Result<u32, MmioError>
pub fn wait_frame(&self, timeout: Duration) -> Result<bool, MmioError>
```

The read channel runs in **park mode**: it repeats one frame buffer, and `show_frame(n)` switches to frame `n` at the next frame
boundary, which is all double buffering needs (draw into the hidden buffer, `show_frame`, wait for a frame boundary, draw into the
other). `start_read(frames, g)` resets the channel, writes the frame buffers' 32-bit physical addresses (`0x5C` + 4 per frame),
the line size, stride and line count (`0x54`, `0x58`, `0x50`, the last starting the transfer) and parks on frame 0.
`Geometry { hsize, stride, lines }` is in bytes and lines. Interrupts: `enable_frame_interrupt` raises one at the end of every
frame; `ack_frame` clears the flag (write 1) and returns what was set; `wait_frame` polls. Register map: `MM2S_CR` `0x00`,
`MM2S_SR` `0x04`, `PARK_PTR` `0x28` (read reference in bits 4:0, the frame being read in bits 20:16), `VERSION` `0x2C`,
`FRMSTORE` `0x18`.
