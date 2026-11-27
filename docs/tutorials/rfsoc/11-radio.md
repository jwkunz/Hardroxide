# RFSoC 11: A tone out and back

The board's reason to exist is its radio: converters that turn numbers into signals up to a few gigahertz, and back.
Here a program sends a tone out of each DAC, brings it in through an ADC, and finds it in the spectrum. It is the same
program on bare metal, on the kernel and under Linux.

## Cables

Two SMA cables, **DAC A to ADC A** and **DAC B to ADC B**. Connect only this board's own DACs to its ADCs: before
connecting anything else to an ADC, check the board's documentation for what its inputs bear.

## The radio in the base overlay

| In the FPGA | What it does | Driver |
|---|---|---|
| the data converter (`radio_rfdc`) | DACs and ADCs at 4915.2 million samples a second, each with a **mixer**: an oscillator that shifts a signal up or down in frequency | [`hardroxide_ip::rfdc`](../../reference/ip-rfdc.md) |
| an amplitude controller in front of each DAC | repeats one number | [`hardroxide_ip::radio`](../../reference/ip-radio.md) |
| a packet generator and two DMAs after each receiver | puts a set number of samples into memory, I and Q | [`hardroxide_ip::radio`](../../reference/ip-radio.md) |

There is no waveform memory. A **tone is made by the DAC's mixer**: the amplitude controller sends a constant, and the
mixer turns it into a sine at the mixer's frequency. On the way in, the ADC's mixer shifts the signal down again, and
what arrives in memory is a pair of numbers, I and Q, for each instant, 2457.6 million of them a second.

With the transmit mixer at `f_tx` and the receive mixer at `-f_rx`, the tone appears at **`f_tx - f_rx`**, which may be
negative: with I and Q together, the two sides of zero are different frequencies. The same real signal also appears, as
strong, at `-(f_tx + f_rx)`, its **image**; the program knows where to expect it.

> **Warning.** The blocks after the converter, and the converters themselves, run on clocks that come from the board's
> clock chips. **Until those are programmed, an access to the blocks never completes** and the board has to be reset or
> switched off. The program programs the chips first (under Linux, the image does while starting), then starts the
> converter's tiles, which fails with an error if there is no clock, and only then touches the rest. Keep that order.

## The heart of the program

From `apps/rfsoc4x2/common/loopback.rs`, shortened:

```rust,ignore
rfdc.start_tile(Converter::Dac, 2)?;                       // DAC A's tile; the others too
rfdc.set_mixer(Converter::Dac, 2, 0, Mixer { frequency_hz: 300_000_000, phase_degrees: 0.0 })?;
amplitude.set_amplitude(0x4000)?;                           // half of full scale
amplitude.enable()?;
rfdc.set_mixer(Converter::Adc, 2, 1, Mixer { frequency_hz: -270_000_000, phase_degrees: 0.0 })?;
let (i, q) = radio::record(generator, i_dma, q_dma, (i, q), 16384, Duration::from_millis(100))?;
// A Fourier transform of I + jQ: the strongest line, apart from the image, is at +30 MHz.
```

A record of 16384 samples lasts 6.7 microseconds, and its spectrum has a line every 150 kHz. The program checks that
each tone is where it should be (also below zero), that half the amplitude reads 6 dB lower, that only the cabled
input hears it, and that it disappears when switched off.

## On bare metal

The board in **JTAG** mode, as in [tutorial 2](02-base-overlay.md). From the repository's root, with the crate of that
tutorial:

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --platform baremetal \
    --name design_1 -o ../base --hardroxide-path .
$ cp apps/rfsoc4x2/baremetal/radio_loopback.rs ../base/examples/
$ cp -r apps/rfsoc4x2/common ../base/examples/
$ cp fixtures/rfsoc4x2/clocks/*.txt ../base/examples/
```

The program programs the clock chips itself, so add the board's chip drivers under `[dev-dependencies]` in
`../base/Cargo.toml`:

```toml
hardroxide-board = { path = "../Hardroxide/crates/hardroxide-board" }
```

```console
$ cd ../base
$ cargo build --release --example radio_loopback
$ hardroxide run target/aarch64-unknown-none/release/examples/radio_loopback \
    --xsa ../Hardroxide/fixtures/board/rfsoc4x2_base_overlay.xsa --watch 40
```

It prints `DAC synthesizer: Locked` and `ADC synthesizer: Locked`, then the lines shown [under Linux](#under-linux),
and `RADIO LOOPBACK OK`.

## On the kernel

`rt_radio_loopback.rs` runs the measurement as a task, under a task that must run every millisecond. Copy it, `common/`
and the register files into the kernel crate of [tutorial 4](04-mood-light.md) (`apps/rfsoc4x2/rtos/`, `../base-rtos`),
add `hardroxide-board` as above, and build and run it the same way.

| Task | Priority | Does |
|---|---|---|
| `control` | 5 | every millisecond: notes how late it is, blinks the processor's first LED |
| `radio` | 3 | the measurement, spectra included |

The urgent task preempts the arithmetic in the middle of a Fourier transform, registers and all. After the same checks
as [under Linux](#under-linux) it prints:

```text
control loop: 692 releases while the radio worked, worst deviation from 1 ms: 2 us
RT RADIO LOOPBACK OK
```

## Under Linux

The board in **SD** mode, as in [tutorial 8](08-linux.md), with
[`u-dma-buf`](../../guide/linux-ultrascale.md#dma-memory-u-dma-buf) loaded (two buffers of at least 32 KiB). Wait two
minutes after switching on: the image's start-up script programs the clock chips, and the program refuses to run until
it has.

```console
$ cp apps/rfsoc4x2/linux/radio_loopback.rs ../base-linux/examples/
$ cp -r apps/rfsoc4x2/common ../base-linux/examples/
$ scripts/board/run.sh ../base-linux radio_loopback
```

```text
== RFSoC 4x2 base overlay: radio loopback ==
converter tiles:
  ok   DAC tile 0: state 15, PLL locked true
  ok   DAC tile 2: state 15, PLL locked true
  ok   ADC tile 0: state 15, PLL locked true
  ok   ADC tile 2: state 15, PLL locked true

DAC A at 300000000 Hz into ADC A (receiver 21):
  ok   receive mixer -270000000 Hz: strongest but the image (-570000000 Hz) at 30000000 Hz (expected 30000000), -18.3 dBFS there, -111.3 dBFS at -30000000 Hz
  ok   receive mixer -345000000 Hz: strongest but the image (-645000000 Hz) at -45000000 Hz (expected -45000000), -18.3 dBFS there, -120.6 dBFS at 45000000 Hz
  ok   receive mixer -198750000 Hz: strongest but the image (-498750000 Hz) at 101250000 Hz (expected 101250000), -18.3 dBFS there, -120.5 dBFS at -101250000 Hz
  ok   receive mixer -307500000 Hz: strongest but the image (-607500000 Hz) at -7500000 Hz (expected -7500000), -18.3 dBFS there, -105.3 dBFS at 7500000 Hz
  ok   half the amplitude: -24.4 dBFS, 6.02 dB less (expected 6.02)
       ADC A (receiver 21): -18.3 dBFS
       ADC B (receiver 20): -99.6 dBFS
       ADC C (receiver 01): -113.5 dBFS
       ADC D (receiver 00): -106.4 dBFS
  ok   only ADC A (receiver 21) hears it (the others at least 30 dB down)
  ok   switched off: -109.4 dBFS, 91.1 dB down (at least 40)
```

The same follows for DAC B into ADC B (at -18.9 dBFS), and `RADIO LOOPBACK OK`. "dBFS" is decibels relative to the
largest signal the numbers can hold; the exact levels depend on your cables.

## Limits

- **Record at most 30720 samples from ADC B** (`radio_receiver_channel_20`). In RFSoC-PYNQ's bitstream its way to memory
  has no data buffer, and longer records lose their end, with PYNQ's own software too
  ([why](../../reference/ip-radio.md#receive)). The other inputs take 32768. `record` reports a record that did not
  arrive whole.
- One receiver is recorded at a time, and nothing ties its start to another's or to a transmitter's.
- The drivers set mixer frequencies and phases; sample rates and everything else stay as the bitstream has them.

## Try this

- Move a cable to ADC C or ADC D: some checks fail, and the list of levels shows who hears the tone.
- Change `TRANSMIT_HZ` and `BASEBAND_HZ` in `common/loopback.rs`. Keep them multiples of 150 kHz, and see what happens
  to the spectrum if you don't.
- Find the image: print the level at `-(f_tx + f_rx)`. Why is it exactly as strong as the tone?
