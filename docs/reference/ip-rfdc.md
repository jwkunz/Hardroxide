# RF data converter

`hardroxide_ip::rfdc` — `xilinx.com:ip:usp_rf_data_converter` (PG269) of a third-generation Zynq UltraScale+ RFSoC,
new in 3.0. It starts a tile, reports its state, and sets a block's mixer frequency and phase. Everything else stays as
the bitstream configured it ([ADR 0006](../decisions/0006-radio.md)).

> **Verified on an RFSoC 4x2**, bare metal and Linux (2026-11-26): starting tiles; frequencies of transmit and receive
> mixers, shown by where a looped-back tone appears; frequency and phase of every block read back by PYNQ's own `xrfdc`
> as written. What the phase does to a signal has not been measured. The register offsets and the order of writes follow
> the vendor's `xrfdc` driver.

```rust,ignore
pub enum Converter
pub const TILES: usize = 4;
pub const BLOCKS: usize = 4;
pub struct TileConfig
pub sample_rate_hz: u64,
pub blocks: u8,
pub fine_mixers: u8,
pub struct RfdcConfig
pub adc: [Option<TileConfig>; TILES],
pub dac: [Option<TileConfig>; TILES],
pub paired_adc: bool,
pub struct TileStatus
pub state: u8,
pub powered: bool,
pub pll_locked: bool,
pub fn running(&self) -> bool
pub struct Mixer
pub frequency_hz: i64,
pub phase_degrees: f32,
pub struct Rfdc<R: Mmio>
pub fn new(regs: R, config: RfdcConfig) -> Self
pub fn regs(&self) -> &R
pub fn config(&self) -> &RfdcConfig
pub fn set_timeout(&mut self, timeout: Duration)
pub fn tile(&self, converter: Converter, tile: usize) -> Result<TileConfig, RfdcError>
pub fn tile_status(
pub fn start_tile(
pub fn set_mixer(
pub fn mixer(
pub enum RfdcError
```

- `new(regs, config)`: `regs` is the converter's whole 256 KiB window. A generated `Design` fills `config` from the XSA:
  the tiles and blocks of the design, their sample rates, which use the fine mixer, and whether the chip pairs its ADCs
  (as the ZU48DR does: two to a tile). All of it is part of the interface hash.
- Tiles and blocks are numbered as in the design's port names: `vin2_23` and receive port `m22` belong to ADC tile 2,
  block 1; `vout20` to DAC tile 2, block 0. On a paired ADC a block is two blocks of registers, which the driver
  handles.
- **Clocks first.** The converters run only once the board's clock chips are programmed
  ([`hardroxide-board`](board.md) on bare metal; PYNQ's boot service under Linux). If the bitstream was loaded before
  that, `start_tile(converter, tile)` restarts the tile and waits until it runs (1 s unless `set_timeout` says
  otherwise). A restart returns the tile's settings to the bitstream's. `tile_status` reads the state machine (15 =
  running), power and PLL lock.
- `set_mixer(converter, tile, block, mixer)` writes frequency and phase and applies them at once. A transmit mixer
  shifts up by `frequency_hz`; to bring a received frequency down to zero, give the receive mixer its negative. The
  limit is half the sample rate either way (half that again for a DAC in the half-bandwidth mode). On a paired ADC the
  update is tile-wide.
- Refused, with a typed [`RfdcError`](#rf-data-converter) and before anything is written: a tile or block that is not
  in the design, a block without a fine mixer, a frequency or phase out of range, a DAC that bypasses its mixer, and a
  tile that is not running.
- `mixer` reads the setting back from the registers (the frequency to the nearest hertz of the stored word).

Not covered: sample rates and the PLL, interpolation and decimation, the coarse mixer, the second Nyquist zone, aligning
several converters, calibration, attenuators, alarms and the converter's interrupts.
