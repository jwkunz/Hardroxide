# Audio codec controller

`hardroxide_ip::audio` — the PYNQ audio codec controller (`xilinx.com:user:audio_codec_ctrl`): an I2S serialiser and deserialiser for the
ADAU1761 codec with an AXI-Lite register window and **no FIFO and no interrupt**: one stereo frame in each direction, so software must move a
frame every sample period (about 20.5 us). The register map and the timing come from the core's VHDL source in the PYNQ repository (Apache-2.0).
The codec chip itself is configured over I2C by [`hardroxide-audio`](audio.md).

> **Not yet run on a board** (see [Verification gaps](../08-verification-gaps.md)): the register access is unit-tested against the mock backend.

```rust,ignore
pub const SAMPLE_RATE_HZ: f64
pub struct Frame
pub fn from_i16(left: i16, right: i16) -> Frame
pub fn to_i16(self) -> (i16, i16)
pub struct AudioCodecCtrl<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn ready(&self) -> Result<bool, MmioError>
pub fn clear_ready(&self) -> Result<(), MmioError>
pub fn read_frame(&self) -> Result<Frame, MmioError>
pub fn write_frame(&self, f: Frame) -> Result<(), MmioError>
pub fn wait_sample(&self, spins: u32) -> Result<bool, MmioError>
```

Registers: `0x00` left receive, `0x04` right receive, `0x08` left transmit, `0x0C` right transmit (24-bit samples in the low bits of a 32-bit word),
`0x10` status: bit 0 is set when a new input frame has arrived and cleared by a write. The bit clock is the AXI clock divided by 32 and the
left/right clock by 2048, so with a 100 MHz AXI clock the sample rate is 100 MHz / 2048 = 48,828.125 Hz (not exactly 48 kHz). `wait_sample(spins)` polls for the next sample tick and clears the flag when it
sees it; `false` means none came within `spins` reads (no clock). Clear the flag once before a stream; a tick already waiting when a loop comes back for it means the loop was late. `Frame` holds 24-bit signed samples sign-extended
in an `i32`; `from_i16`/`to_i16` convert from and to 16 bits.
