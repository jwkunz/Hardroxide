# ADR 0006: the radio driver (3.1)

Status: **accepted** 2026-11-26 (the decisions under "Decided" are the maintainer's; the rest may be revised as work
proceeds). Builds on
[ADR 0005](0005-ultrascale-plus.md), which sets 3.1 as the RF data converter driver with a cable loopback, accepted when
captured samples match the generated signal.

## Decided

| | Question | Decision |
|---|---|---|
| A | Must sending and recording start on the same clock? | **No, not in 3.1.** The base overlay cannot do it and the cable test does not need it. A later design change could add it |
| B | Must a program send any waveform, or are tones enough? | **Tones.** The base overlay sends nothing else, and the examples use the base overlay. Waveforms would need our own bitstream |
| C | How much of the converter's settings? | **Mixer frequency and phase, and status**, with the rest left as the bitstream set it (list under [Not in 3.1](#not-in-31)) |

## What the base overlay's radio is

Read from `base.hwh` in `fixtures/board/rfsoc4x2_base_overlay.xsa` and from RFSoC-PYNQ's drivers in the 4.0.0 image
(`rfsystem`, BSD 3-Clause).

| Part | What it is |
|---|---|
| Converters | 4915.2 MS/s, fine mixer, rates and data formats fixed by the bitstream |
| 2 transmitters | DAC tile 0 and tile 2, block 0. In front of each, `amplitude_controller`: repeats one I/Q value (register `0x00` enable, `0x04` value). **The tone is made by the DAC's mixer**; there is no waveform memory |
| 4 receivers | ADC tiles 0 and 2, two each; interpolation and decimation by 2. Each gives an I stream and a Q stream at 2457.6 MS/s |
| Recording | per receiver, `packet_generator` (`0x00` length in words of 8 samples, 2 to 4096; `0x04` a rising edge sends one packet) and two DMAs, one for I and one for Q |
| Interrupts | the DMAs' through `axi_intc_0` |

So one recording is up to 32768 I/Q samples (13.3 us) of one receiver, its I and Q taken together. Receivers are started
one by one, and nothing ties them to each other or to the transmitters.

Which socket is which: from the loopback design, `vout20` is DAC A, `vout00` is DAC B, `vin2_23` is ADC A and `vin2_01`
ADC B. That makes the base overlay's receiver 21 ADC A and receiver 20 ADC B. ADC C is receiver 01 and ADC D receiver
00, found in R3 by moving the cables.

Today the generator gives the DMAs their driver; the converter, the amplitude controllers and the packet generators are
raw register windows.

## What gets built

### 1. Converter driver: `hardroxide_ip::rfdc`

For `xilinx.com:ip:usp_rf_data_converter`, written from the register definitions of the vendor's `xrfdc` driver (MIT, in
the Vitis tools).

- Start a tile and wait for it; report its state and whether its PLL locked. A tile that does not start gives a typed
  error that names the likely cause, the board's clock chips.
- Set a block's mixer frequency (in Hz, negative allowed) and phase, apply it, and read it back.
- The tiles, blocks, sample rates and mixer types come from the design (`driver_config()`, so they are in the interface
  hash). A setting the bitstream does not support is refused, not written.
- On this chip each receiver is **two hardware blocks** in the registers (`xrfdc` writes both); the driver hides that.

### 2. The overlay's two small blocks: `hardroxide_ip::radio`

`AmplitudeController` (value, on and off) and `PacketGenerator` (length, send), and one function that records a receiver:
arm both DMAs, then send the packet, then wait for both. Doing it in another order loses the data or never finishes.

### 3. Generator

The three block types get these drivers in generated crates, in place of raw windows. No released version generates
UltraScale+ crates, and the Zynq-7000 has none of these blocks, so the 2.7 compatibility promise is not touched.

### 4. Programs

A loopback program on the base overlay for each of bare metal, the kernel and Linux (`apps/rfsoc4x2/`), with tutorials.
It sends a tone from each DAC, records it, finds it in the spectrum and says how strong it is and how far from where it
should be. The spectrum code stays in the program; it is not a library.

The clock chips: bare metal and kernel programs program them first (`hardroxide-board`). Linux programs rely on PYNQ's
boot service, as [tutorial 10](../tutorials/rfsoc/10-linux-memory.md) does, and refuse to go on if it did not succeed.

## How it is accepted

On the host, with a mock register backend: the register sequences, the frequency arithmetic and its limits, and the
refusals.

On the board, SMA cables from DAC A to ADC A and from DAC B to ADC B:

1. Transmit mixer at `f_tx`, receive mixer at `-f_rx`: the recorded tone is at `f_tx - f_rx`, within one bin of the
   spectrum (75 kHz in a record of 32768 samples; the program records 16384, 150 kHz, see R3 below), for several pairs, **negative differences among them**, with nothing comparable at minus that
   frequency (which shows that I and Q are not swapped). A real signal received through a mixer also appears, as strong,
   at `-(f_tx + f_rx)`; the check leaves that bin out.
2. Halving the transmitted value lowers the tone by 6 dB; switching it off removes it.
3. Each tone arrives on its own input and not, at a comparable level, on the other.
4. **Checked against the vendor's driver**: on Linux, settings written by ours are read back with PYNQ's `xrfdc`.
5. ADC C and D: the maintainer moves a cable, which also settles which receivers they are.

## Steps

| Step | Deliverable | Needs the board |
|---|---|---|
| R1 | converter driver and host tests | no |
| R2 | the two block drivers, the generator, updated golden snapshots | no |
| R3 | the loopback programs; acceptance on bare metal, the kernel and Linux | yes |
| R4 | reference pages and tutorials; then the release's full regression ([ADR 0005](0005-ultrascale-plus.md), decision 8) | yes |

R1 and R2 are written and pass their host tests (2026-11-26). Of R3, the loopback program is written for bare metal
and Linux (`apps/rfsoc4x2/{baremetal,linux}/radio_loopback.rs`, shared measurement in `common/`); the kernel's is still
to be written.

The kernel's program (`rtos/rt_radio_loopback.rs`, the measurement under a 1 ms task) and the tutorial
(`tutorials/rfsoc/11-radio.md`) are written; the kernel's has not yet run on the board, and the bare-metal one has not
since its wiring and clock code moved into `common/`.

**R3 on the board, bare metal** (RFSoC 4x2 over JTAG, 2026-11-26, cables DAC A to ADC A and DAC B to ADC B): acceptance
1 to 3 passed. The four tiles started; each tone was at `f_tx - f_rx` for both signs, at -18.3 dBFS (DAC A) and -18.6
dBFS (DAC B), with nothing above -105 dBFS at minus that frequency; half the value gave 6.02 and 6.03 dB less; the other
three receivers were at least 77 dB down; switched off, the tone fell by more than 90 dB. **Acceptance 5**, the same day: with
the cables moved to ADC C and ADC D, the tones were heard by receivers 01 and 00, and the same checks passed there
(-18.2 and -18.5 dBFS, 6.02 dB for half, at least 77 dB between inputs, off more than 80 dB).

**R3 on the board, Linux** (PYNQ 4.0.0, from the board's memory, 2026-11-26): the loopback program passed the same
checks on ADC A and B (-18.3 and -18.9 dBFS). **Acceptance 4:** a different frequency and phase written by `rfdc` to
each of the six blocks was read back by the image's `xrfdc` with the same frequency to the hertz and the same phase to
four decimals, and with the fine mixer, complex-to-real on the DACs and real-to-complex on the ADCs.

**Found: receiver 20 (ADC B) cannot take a full-length record.** Over JTAG with raw register accesses, from a clean
state, 20 tries per length: receivers 00, 01 and 21 never failed up to 4096 words; receiver 20 never up to 3840 words
(30720 samples) and every time from 4000. The last 4 KiB did not reach memory, a DMA waited forever, and the left-over
words then made the next transfer on that DMA end early. In `base.hwh` receiver 20's path to memory is the only one of
the four without a data FIFO (`S00_HAS_DATA_FIFO`), which is the likely cause; **RFSoC-PYNQ's own software shows
it too** (2026-11-26: its overlay and drivers, five tries each): receiver 20 stalled every time at 4096 words and never
at 3840; receiver 21 stalled at neither. `radio::record` now discards left-over data before asking for a packet and refuses a short result;
on the board, after each of five failed full-length records the next record was correct.

No new bitstream is needed. `fixtures/rfsoc4x2/rfsoc4x2_rf_loopback.xsa` (waveform playback, two inputs recorded
together) stays a test of the converters with raw register access; its own blocks get no driver.

## Not in 3.1

Changing sample rates, the PLL, interpolation or decimation while running; the coarse mixer; the second Nyquist zone;
aligning several converters (multi-tile synchronization); calibration, gain and offset correction, attenuators,
threshold and over-range alarms; the converters' interrupts; sending waveforms on the base overlay; a common start; other
boards.

## Risks

| Risk | Mitigation |
|---|---|
| The vendor's software sets more than the mixer (Nyquist zone, FIFO enable, calibration mode); the bitstream's defaults may not suit receiving through the mixer | the tiles locked and tones arrived with defaults in the loopback design, which receives without a mixer; check on the board early in R3, and compare the registers with a board PYNQ has set up |
| Our register use is wrong in a way our own reads cannot show | acceptance 1 (an independent signal) and 4 (an independent reader) |
| The board's analog path attenuates some frequencies | choose test frequencies near those already measured (100 to 600 MHz at the converter), and report levels instead of requiring absolute ones |
| A loud signal damages an input | cables only between this board's own DACs and ADCs, at the levels already used |
