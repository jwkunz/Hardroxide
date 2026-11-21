# Applications 3: Sound

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 1](01-base-lights.md), headphones in the **HPH OUT** jack and, for the recording programs, a source on **LINE IN**

> **Status: written, not yet run on a board.** Everything below compiles and the register-level code (the codec's I2C sequences, the I2S controller, WAV parsing, the
> synthesis and resampling) is unit-tested on the host, but nothing has played a sound yet. The page will get real output after the first run
> ([Verification gaps](../../08-verification-gaps.md)).

The base overlay's audio path is the ADAU1761 codec chip, configured over I2C, and a small controller in the fabric that exchanges one stereo sample at a
time with it over I2S. There is no FIFO and no interrupt: the processor must move a frame every 20.5 microseconds (48.828 kHz), which makes audio the
sharpest real-time job in the overlay.

## 1. The pieces

| Piece | Where | What it does |
|---|---|---|
| `ps7::i2c::I2c` | [`hardroxide-ps7`](../../reference/ps7.md#i2c-controllers) | the PS I2C 1 controller, routed through the EMIO to the codec's control pins |
| `AudioCodecCtrl` | [`hardroxide-ip`](../../reference/ip-audio.md) | the I2S controller: a frame in, a frame out, a "new sample" flag |
| `Adau1761` | [`hardroxide-audio`](../../reference/audio.md) | the codec's PLL, converters, mixers, volume and input selection |
| `Wav`, `Note`, `Resample`, `play`, `record` | `hardroxide-audio` | files, synthesis, rate conversion, moving frames |

## 2. Set up and play a tune

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal -o work/base --hardroxide-path . --name design_1
$ cd work/base
$ cargo add --dev --path ../../crates/hardroxide-audio --no-default-features
$ cp ../../apps/base/baremetal/audio_tone.rs examples/
$ cargo build --release --example audio_tone
$ hardroxide run target/armv7a-none-eabihf/release/examples/audio_tone --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 20
```

`audio_tone` brings up the I2C clock, the codec (it prints whether the PLL locked), plays a C major scale and then a chord, and prints for each note how many
frames moved and how many sample ticks it missed. The expected output is `0 missed` throughout: the loop has 20 microseconds per sample and a 650 MHz core.

```rust,ignore
let i2c = ps7::i2c::I2c::new(be.map(ps7::map::I2C1_BASE as u64, ps7::map::I2C_SIZE).unwrap());
i2c.set_clock(ps7::clocks::cpu_1x_hz(&slcr, 50_000_000).unwrap() as u32, 400_000).unwrap();
let mut codec = Adau1761::new(i2c);
codec.init().unwrap();              // PLL to 49.152 MHz, converters, serial port: true if the PLL locked
codec.enable_playback(50).unwrap(); // unmute the headphone amplifier (volume 0..62)
let stats = play(&d.audio_codec_ctrl_0, Note::at_codec_rate(69, 500)).unwrap();   // A4 for half a second
```

## 3. Play a WAV file

`apps/base/baremetal/audio_wav.rs` includes `apps/base/data/chime.wav` (8 kHz, mono, 16 bits, two seconds, made by `make_chime.py` from nothing but sine waves: a rising
arpeggio), parses it, resamples it to the codec's 48.828 kHz and plays it twice. Copy both the program and the data file into the crate's `examples/`:

```console
$ cp ../../apps/base/baremetal/audio_wav.rs ../../apps/base/data/chime.wav examples/
$ cargo build --release --example audio_wav
$ hardroxide run target/armv7a-none-eabihf/release/examples/audio_wav --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 20
```

Your own file works the same way as long as it is a PCM WAV with 1 or 2 channels and 8, 16 or 24 bits; put it in `examples/` and change the `include_bytes!`. At 44.1 kHz stereo
a minute of audio is 10 MB, so a long file belongs on the SD card (the next version in the series).

## 4. Record and play back, and the pass-through

`audio_record.rs` records five seconds from the line input into a 2 MiB buffer in memory, prints the peak level of each channel (a quick way to see that a source is
connected and not clipping), and plays it back. `audio_loopback.rs` passes the line input to the headphones through the processor for 30 seconds, halving the volume on the way:
one frame every 20.5 microseconds. Both report missed samples.

## 5. What to check, and what to tell us

When you run these for the first time: does the console say the PLL locked? Is there sound, at the right pitch (the controller's rate is 48.828 kHz, the codec's PLL is set for
49.152 MHz = 1024 x 48 kHz; a pitch error of 0.3 % would be inaudible, a large one would mean the clocks are not what we think)? Are there clicks (missed samples)? Does the line
input record, and at what level? The answers go into [Verification gaps](../../08-verification-gaps.md) and this page.

## 6. Under the kernel

Not yet. The controller needs a frame every 20.5 microseconds, and the kernel's tick is a millisecond, so a task cannot pace it. The plan is an interrupt-driven pump: a PS timer
interrupting at twice the sample rate whose handler moves a frame when the "new sample" flag is up, feeding a ring buffer that ordinary tasks fill. That costs about 15 % of the processor
and keeps the other tasks' deadlines; it is the next step for audio in the RTOS flavor.
