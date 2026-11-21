# Applications 3: Sound

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 1](01-base-lights.md), headphones in the **HPH OUT** jack and, for the recording programs, a source on **LINE IN**

> **Status: run on a PYNQ-Z2 (2026-11-21).** Playing tones and a WAV file, recording, and the full-duplex loop all work, with no missed samples. Two programs need a separate
> source and headphones and have **not** been run: `audio_record` and `audio_loopback`. The first run found a bug in the I2C driver (section 7).

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
frames moved and how many sample ticks it missed:

```console
== audio_tone ==
I2C clock 378787 Hz
codec up (PLL locked)
note 60: 12207 frames, 0 missed
note 62: 12207 frames, 0 missed
...
note 72: 12207 frames, 0 missed
chord: 58593 frames, 0 missed
done
```

The scale and the chord were heard through headphones in the **HPH OUT** jack. The loop has 20 microseconds per sample and a 650 MHz core, and misses none.

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

```console
== audio_wav: 8000 Hz, 1 channel(s), 16 bits, 2000 ms ==
pass 1: 97661 frames played, 0 missed
pass 2: 97661 frames played, 0 missed
done
```

The arpeggio played cleanly twice, at the right pitch. Your own file works the same way as long as it is a PCM WAV with 1 or 2 channels and 8, 16 or 24 bits; put it in `examples/` and change the `include_bytes!`. At 44.1 kHz stereo
a minute of audio is 10 MB, so a long file belongs on the SD card (the next version in the series).

## 4. Record and play back: a self-test with one cable

With a cable from the green **HPH OUT** jack to the blue **LINE IN** jack the board can test itself (the pink MIC jack is a differential microphone input and did not hear the tone):

- `audio_loopback_test.rs` plays a 1 kHz sine while recording the input in the same sample loop (the controller is full duplex), one second of silence and then two seconds of tone:

```console
== audio_loopback_test: HPH OUT cabled to the input ==
0 samples missed in 146484 frames
left: silence peak 1.52 %, tone peak 10.7 % rms 7.5 %, 1000 Hz (1500 crossings in 1.50 s)
right: silence peak 8.95 %, tone peak 11.1 % rms 7.6 %, 1000 Hz (1500 crossings in 1.50 s)
LOOPBACK OK
```

- `audio_record_test.rs` records the tone into memory, plays that recording back out while recording again, and checks both recordings for the tone:

```console
== audio_record_test: HPH OUT cabled to LINE IN ==
1. tone played and recorded: 97656 frames, 0 missed; recording holds 1000 Hz at 10.7 % peak
2. recording played back and recorded again: 97656 frames, 0 missed; 999 Hz at 2.4 % peak
RECORD/PLAYBACK OK
```

The frequency is counted against the program's own sample clock, so it proves the loop is consistent end to end, not the absolute rate; the absolute pitch was judged by ear (the scale and the chime sounded right).

`audio_record.rs` (record five seconds from the line input, print the peak levels, play it back) and `audio_loopback.rs` (line in to headphones through the processor, 30 seconds) need a separate source and
headphones and **have not been run**; the loops they use are the ones the self-tests exercise.

## 5. The sample rate

The controller's rate is 100 MHz / 2048 = 48.828 kHz and the codec's PLL is set for 1024 x 48 kHz = 49.152 MHz; the two are not locked to each other by the programs and the I2S link
works regardless (the converters run from the clocks the controller supplies). Pitch is therefore 0.1 % off a nominal 48 kHz file's, which is inaudible.

## 6. Under the kernel

Not yet. The controller needs a frame every 20.5 microseconds, and the kernel's tick is a millisecond, so a task cannot pace it. The plan is an interrupt-driven pump: a PS timer
interrupting at twice the sample rate whose handler moves a frame when the "new sample" flag is up, feeding a ring buffer that ordinary tasks fill. That costs about 15 % of the processor
and keeps the other tasks' deadlines; it is the next step for audio in the RTOS flavor.

## 7. What the first run taught us

1. **A stack overflow that the host tests could not see.** The first run printed the I2C clock and then took an exception in a loop. The cause (found by reading the CPU's banked registers over JTAG: the
   system-mode stack pointer had wrapped to `0xFFFEFFF0` and the abort was a push at the entry of `I2c::transaction`) was unbounded recursion: in `impl embedded_hal::i2c::I2c for I2c<R>`, `transaction`
   called `self.write(..)`, which Rust resolved to the *trait's* default `write` (that calls `transaction`) rather than the controller's own method. The host tests only used the inherent methods. Fixed, with a test that goes through the trait.
2. **The "missed samples" counter** first reported every frame as missed: the loop checked the ready flag before clearing it. The flag is now cleared when a tick is consumed, so a tick already waiting means the loop was genuinely late.
3. **The MIC jack** is a differential microphone input; a cable from the headphone output did not register there. The line input did.
4. After these, the codec came up on the first attempt (PLL locked), which also confirms that the PS I2C 1 controller reaches the codec through the EMIO.
