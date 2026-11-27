# hardroxide-audio

Audio for the PYNQ base overlay: the ADAU1761 codec over I2C (clock PLL, mixers, volume, input selection), WAV files
(parse and write), tone synthesis, resampling, and sample-by-sample playback and recording through the overlay's I2S
controller. `no_std`, no allocation.

This is a crate of [Hardroxide](https://github.com/jwkunz/Hardroxide): a generator and runtime that turns a Vivado XSA into a typed
Rust HAL crate for Zynq-7000 and Zynq UltraScale+ (tested on a PYNQ-Z2 and an RFSoC 4x2). The user guide and API
reference are at <https://jwkunz.github.io/Hardroxide/>.

Licensed under either of Apache-2.0 or MIT at your option.
