# hardroxide-audio

Audio for the PYNQ base overlay. `no_std`, no allocation.

> **Verified on a PYNQ-Z2 (2026-11-21):** tones, WAV playback, a full-duplex line-in loop and recording to memory, with no missed samples. The register sequences follow PYNQ's audio driver and the
> controller's VHDL source (Apache-2.0). Not run: the microphone input and the pass-through with a separate source. See [Verification gaps](../08-verification-gaps.md).

| Module | What it is |
|---|---|
| `adau1761` | the ADAU1761 codec chip, configured over any `embedded-hal` I2C bus |
| `wav` | parse WAV files in memory; write a header |
| `tone` | a sine oscillator, notes with a click-free envelope, MIDI pitches |
| `resample` | linear resampling of a frame stream |
| `io` | moving frames through the I2S controller ([`hardroxide_ip::audio`](ip-audio.md)): playback, recording, pass-through, mixing |

```rust,ignore
pub const ADDRESS: u8 = 0x3B;
pub enum Input
pub struct Adau1761<I: I2c>
pub fn new(i2c: I) -> Self
pub fn release(self) -> I
pub fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), I::Error>
pub fn read_regs(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), I::Error>
pub fn start_pll(&mut self, polls: u32) -> Result<bool, I::Error>
pub fn configure(&mut self) -> Result<(), I::Error>
pub fn init(&mut self) -> Result<bool, I::Error>
pub fn select_input(&mut self, input: Input) -> Result<(), I::Error>
pub fn enable_playback(&mut self, volume: u8) -> Result<(), I::Error>
pub fn set_volume(&mut self, volume: u8) -> Result<(), I::Error>
pub fn mute(&mut self) -> Result<(), I::Error>
pub fn enable_bypass(&mut self, volume: u8) -> Result<(), I::Error>
```

**The codec.** Each control-register write is three bytes: `0x40`, the register, the value. `init` programs the PLL from the 10 MHz master clock the board
supplies to 49.152 MHz (R + N/M = 4 + 572/625), waits for the lock (`Ok(false)` if it never locks), then sets the converters, mixers, serial port (the codec is the
I2S slave) and clocks. `select_input(Input::LineIn | Microphone | None)` chooses the record source; `enable_playback(volume)` unmutes the DACs into
the headphone amplifier (`volume` 0 = -57 dB to 62 = +5 dB); `mute` silences it; `enable_bypass` routes the input to the output inside the codec.

```rust,ignore
pub enum WavError
pub struct Wav<'a>
pub fn parse(bytes: &'a [u8]) -> Result<Wav<'a>, WavError>
pub fn frame_bytes(&self) -> usize
pub fn len(&self) -> usize
pub fn is_empty(&self) -> bool
pub fn millis(&self) -> u64
pub fn frames(&self) -> Frames<'a>
pub fn header_16bit(sample_rate: u32, channels: u16, data_len: u32) -> [u8; 44]
```

**WAV.** `Wav::parse` accepts RIFF/WAVE PCM files with 1 or 2 channels and 8, 16 or 24 bits (unknown chunks are skipped, odd chunk sizes padded, a data
length past the end means "to the end"); `frames()` yields controller frames (24-bit stereo; mono is duplicated, 8-bit unsigned is centred). `header_16bit` writes
the 44-byte header for a recording.

```rust,ignore
pub struct Oscillator
pub fn new(freq_hz: f32, rate_hz: f32) -> Self
pub fn set_freq(&mut self, freq_hz: f32, rate_hz: f32)
pub fn next_sample(&mut self) -> i16
pub struct Note
pub fn midi_hz(n: u8) -> f32
pub fn at_codec_rate(midi: u8, millis: u32) -> Self
pub struct Resample<I: Iterator<Item = Frame>>
```

**Tones and resampling.** `Oscillator` is a 256-point sine table with a 32-bit phase accumulator and linear interpolation. `Note` is an iterator of frames: a
MIDI pitch (69 = A4 = 440 Hz) for a number of samples with a 5 ms linear attack and release. `Resample::new(source, from_hz, to_hz)` interpolates linearly in 16.16 fixed point;
playing a 44.1 or 8 kHz file at the codec's 48.828 kHz goes through it.

```rust,ignore
pub struct Stats
pub fn play<R: Mmio>(ctrl: &AudioCodecCtrl<R>, frames: impl Iterator<Item = Frame>) -> Result<Stats, MmioError>
pub fn record<R: Mmio>(ctrl: &AudioCodecCtrl<R>, out: &mut [Frame]) -> Result<Stats, MmioError>
pub fn play_record<R: Mmio>(ctrl: &AudioCodecCtrl<R>, frames: impl Iterator<Item = Frame>, out: &mut [Frame]) -> Result<Stats, MmioError>
pub fn mix(a: Frame, b: Frame) -> Frame
pub fn scale(f: Frame, gain: u32) -> Frame
```

**Moving samples.** `play`, `record` and `pass_through` poll the controller once per sample, so they must run every ~20.5 us: on bare metal a loop that does nothing else,
under the kernel an interrupt or a dedicated task. `Stats { frames, missed }` reports how many frames moved and how many sample ticks were missed because the loop came back
too late. `play_record` plays and records in the same loop (the controller is full duplex), which with a cable from the output to the input records what was played. A stopped controller (no I2S clock) ends the loop instead of hanging it. `mix` adds two frames saturating at 24 bits; `scale(frame, gain)` multiplies by `gain / 256`.
