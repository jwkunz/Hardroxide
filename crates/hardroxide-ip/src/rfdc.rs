//! RF data converter (`xilinx.com:ip:usp_rf_data_converter`, PG269) of a Zynq UltraScale+
//! RFSoC, third generation: start a tile, report its state, and set a block's mixer frequency
//! and phase. Everything else stays as the bitstream configured it.
//!
//! Register offsets and the order of writes follow the vendor's `xrfdc` driver (MIT).

use core::time::Duration;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{Mmio, MmioError};

/// Analog to digital (receive) or digital to analog (transmit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Converter {
    Adc,
    Dac,
}

impl core::fmt::Display for Converter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Converter::Adc => "ADC",
            Converter::Dac => "DAC",
        })
    }
}

/// Tiles of each kind, and blocks in a tile.
pub const TILES: usize = 4;
pub const BLOCKS: usize = 4;

// A tile's control and status registers, and its blocks' registers.
const DAC_CONTROL: usize = 0x4000;
const ADC_CONTROL: usize = 0x14000;
const DAC_BLOCKS: usize = 0x6000;
const ADC_BLOCKS: usize = 0x16000;
const TILE_STRIDE: usize = 0x4000;
const BLOCK_STRIDE: usize = 0x400;

// Control and status.
const RESTART: usize = 0x04;
const RESTART_STATES: usize = 0x08;
const CURRENT_STATE: usize = 0x0C;
const COMMON_STATUS: usize = 0x228;
const STATUS_POWERED: u32 = 1 << 2;
const STATUS_PLL_LOCKED: u32 = 1 << 3;
const STATE_RUNNING: u32 = 15;
/// Restart the state machine from state 1 to state 15.
const FULL_RESTART: u32 = (1 << 8) | STATE_RUNNING;

// A block.
const ADC_UPDATE: usize = 0x01C;
const DAC_UPDATE: usize = 0x020;
const DAC_DATAPATH: usize = 0x034;
const NCO_UPDATE_MODE: usize = 0x08C;
const NCO_FREQ_UPPER: usize = 0x094;
const NCO_FREQ_MIDDLE: usize = 0x098;
const NCO_FREQ_LOWER: usize = 0x09C;
const NCO_PHASE_UPPER: usize = 0x0A0;
const NCO_PHASE_LOWER: usize = 0x0A4;
const ADC_NCO_PHASE_MODE: usize = 0x0A8;
const UPDATE_SOURCE_MASK: u32 = 0x7;
const UPDATE_IMMEDIATE: u32 = 0;
const UPDATE_TILE: u32 = 2;
const UPDATE_NCO: u32 = 0x2;
/// Triggers a tile-wide update, from the start of the tile's block registers.
const TILE_UPDATE: usize = 0x1C00 + 0x0B8;

/// What the design says about one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileConfig {
    pub sample_rate_hz: u64,
    /// Bit `n` set: block `n` is in the design.
    pub blocks: u8,
    /// Bit `n` set: block `n` has its fine mixer in use, so its frequency can be set.
    pub fine_mixers: u8,
}

/// What the design says about the converter (generated crates pass it from the XSA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfdcConfig {
    pub adc: [Option<TileConfig>; TILES],
    pub dac: [Option<TileConfig>; TILES],
    /// The chip has two fast ADCs per tile rather than four: each ADC block here is two blocks
    /// of registers.
    pub paired_adc: bool,
}

/// A tile's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileStatus {
    /// The power-up state machine: 15 when the tile runs.
    pub state: u8,
    pub powered: bool,
    pub pll_locked: bool,
}

impl TileStatus {
    pub fn running(&self) -> bool {
        u32::from(self.state) == STATE_RUNNING
    }
}

/// A mixer setting: the frequency of its oscillator and the phase it starts with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mixer {
    /// At most half the sample rate either way. A transmit mixer shifts up by this much; give a
    /// receive mixer the negative of a frequency to bring that frequency down to zero.
    pub frequency_hz: i64,
    /// More than -180 and less than 180.
    pub phase_degrees: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum RfdcError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("{0} tile {1} is not in this design")]
    NoTile(Converter, usize),
    #[error("{0} tile {1} has no block {2} in this design")]
    NoBlock(Converter, usize, usize),
    #[error("{0} tile {1} block {2} does not use its fine mixer in this design")]
    NoFineMixer(Converter, usize, usize),
    #[error("mixer frequency {hz} Hz is beyond half the sample rate ({limit} Hz either way)")]
    Frequency { hz: i64, limit: u64 },
    #[error("mixer phase {0} degrees is not between -180 and 180")]
    Phase(f32),
    #[error("DAC tile {0} block {1} bypasses its mixer")]
    Bypassed(usize, usize),
    #[error("{0} tile {1} is not running (state {2} of 15): start it first")]
    NotRunning(Converter, usize, u8),
    #[error(
        "{converter} tile {tile} did not start: state {} of 15, PLL {}. The converters need the board's clock chips \
         to be programmed",
        status.state,
        if status.pll_locked { "locked" } else { "not locked" }
    )]
    Timeout {
        converter: Converter,
        tile: usize,
        status: TileStatus,
    },
}

/// The 48-bit frequency word: the frequency as a signed fraction of the sample rate, rounded
/// toward zero as the vendor's driver does.
fn frequency_word(hz: i64, sample_rate_hz: u64) -> u64 {
    let word = (i128::from(hz) << 48) / i128::from(sample_rate_hz);
    word as u64 & 0xFFFF_FFFF_FFFF
}

/// The frequency a word stands for, to the nearest hertz.
fn word_frequency(word: u64, sample_rate_hz: u64) -> i64 {
    let signed = ((word << 16) as i64) >> 16;
    let scaled = i128::from(signed) * i128::from(sample_rate_hz);
    ((scaled + (1 << 47)) >> 48) as i64
}

/// The 18-bit phase word: 2^17 stands for 180 degrees.
fn phase_word(degrees: f32) -> u32 {
    ((f64::from(degrees) * f64::from(1u32 << 17) / 180.0) as i32) as u32 & 0x3_FFFF
}

fn word_phase(word: u32) -> f32 {
    let signed = ((word << 14) as i32) >> 14;
    (f64::from(signed) * 180.0 / f64::from(1u32 << 17)) as f32
}

pub struct Rfdc<R: Mmio> {
    regs: R,
    config: RfdcConfig,
    timeout: Duration,
}

impl<R: Mmio> Rfdc<R> {
    /// `regs`: the converter's whole 256 KiB window.
    pub fn new(regs: R, config: RfdcConfig) -> Self {
        Self {
            regs,
            config,
            timeout: Duration::from_secs(1),
        }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    pub fn config(&self) -> &RfdcConfig {
        &self.config
    }

    /// How long [`start_tile`](Self::start_tile) waits (1 s unless set).
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// The design's settings of a tile.
    pub fn tile(&self, converter: Converter, tile: usize) -> Result<TileConfig, RfdcError> {
        let tiles = match converter {
            Converter::Adc => &self.config.adc,
            Converter::Dac => &self.config.dac,
        };
        tiles
            .get(tile)
            .copied()
            .flatten()
            .ok_or(RfdcError::NoTile(converter, tile))
    }

    fn control(converter: Converter, tile: usize) -> usize {
        let base = match converter {
            Converter::Adc => ADC_CONTROL,
            Converter::Dac => DAC_CONTROL,
        };
        base + tile * TILE_STRIDE
    }

    fn blocks(converter: Converter, tile: usize) -> usize {
        let base = match converter {
            Converter::Adc => ADC_BLOCKS,
            Converter::Dac => DAC_BLOCKS,
        };
        base + tile * TILE_STRIDE
    }

    pub fn tile_status(&self, converter: Converter, tile: usize) -> Result<TileStatus, RfdcError> {
        self.tile(converter, tile)?;
        let at = Self::control(converter, tile);
        let state = self.regs.read32(at + CURRENT_STATE)? & 0xF;
        let status = self.regs.read32(at + COMMON_STATUS)?;
        Ok(TileStatus {
            state: state as u8,
            powered: status & STATUS_POWERED != 0,
            pll_locked: status & STATUS_PLL_LOCKED != 0,
        })
    }

    /// Restart a tile and wait until it runs. Needed after the board's clocks were programmed
    /// with the bitstream already loaded. Its settings return to the bitstream's.
    pub fn start_tile(
        &mut self,
        converter: Converter,
        tile: usize,
    ) -> Result<TileStatus, RfdcError> {
        self.tile(converter, tile)?;
        let at = Self::control(converter, tile);
        self.regs
            .modify32(at + RESTART_STATES, |v| (v & !0xFFFF) | FULL_RESTART)?;
        self.regs.write32(at + RESTART, 1)?;
        let start = Instant::now();
        loop {
            // The time is taken first, so that a late last look still counts.
            let late = start.elapsed() >= self.timeout;
            let restarting = self.regs.read32(at + RESTART)? & 1 != 0;
            let status = self.tile_status(converter, tile)?;
            if !restarting && status.running() {
                return Ok(status);
            }
            if late {
                return Err(RfdcError::Timeout {
                    converter,
                    tile,
                    status,
                });
            }
            core::hint::spin_loop();
        }
    }

    /// The tile's settings, if the block is there and has a mixer to set.
    fn mixer_block(
        &self,
        converter: Converter,
        tile: usize,
        block: usize,
    ) -> Result<TileConfig, RfdcError> {
        let config = self.tile(converter, tile)?;
        if block >= BLOCKS || config.blocks & (1 << block) == 0 {
            return Err(RfdcError::NoBlock(converter, tile, block));
        }
        if config.fine_mixers & (1 << block) == 0 {
            return Err(RfdcError::NoFineMixer(converter, tile, block));
        }
        Ok(config)
    }

    /// The blocks of registers behind a block: two for a paired ADC.
    fn register_blocks(&self, converter: Converter, block: usize) -> core::ops::Range<usize> {
        if converter == Converter::Adc && self.config.paired_adc {
            2 * block..2 * block + 2
        } else {
            block..block + 1
        }
    }

    /// The rate the block's oscillator runs at, which its frequency is a fraction of.
    fn mixer_rate(
        &self,
        converter: Converter,
        tile: usize,
        block: usize,
        config: TileConfig,
    ) -> Result<u64, RfdcError> {
        if converter == Converter::Adc {
            return Ok(config.sample_rate_hz);
        }
        let at = Self::blocks(converter, tile) + block * BLOCK_STRIDE;
        match self.regs.read32(at + DAC_DATAPATH)? & 3 {
            2 => Ok(config.sample_rate_hz / 2),
            3 => Err(RfdcError::Bypassed(tile, block)),
            _ => Ok(config.sample_rate_hz),
        }
    }

    /// Set a block's mixer frequency and phase, taking effect at once.
    ///
    /// On a paired ADC the change is applied by a tile-wide update, which also applies anything
    /// else that tile's blocks have waiting for one.
    pub fn set_mixer(
        &mut self,
        converter: Converter,
        tile: usize,
        block: usize,
        mixer: Mixer,
    ) -> Result<(), RfdcError> {
        let config = self.mixer_block(converter, tile, block)?;
        // A comparison with NaN is false, so it is refused too.
        if !(mixer.phase_degrees > -180.0 && mixer.phase_degrees < 180.0) {
            return Err(RfdcError::Phase(mixer.phase_degrees));
        }
        let status = self.tile_status(converter, tile)?;
        if !status.running() {
            return Err(RfdcError::NotRunning(converter, tile, status.state));
        }
        let rate = self.mixer_rate(converter, tile, block, config)?;
        let limit = rate / 2;
        if mixer.frequency_hz.unsigned_abs() > limit {
            return Err(RfdcError::Frequency {
                hz: mixer.frequency_hz,
                limit,
            });
        }
        let frequency = frequency_word(mixer.frequency_hz, rate);
        let phase = phase_word(mixer.phase_degrees);
        // A paired ADC cannot apply a change at once, only with its tile.
        let source = if converter == Converter::Adc && self.config.paired_adc {
            UPDATE_TILE
        } else {
            UPDATE_IMMEDIATE
        };
        for b in self.register_blocks(converter, block) {
            let at = Self::blocks(converter, tile) + b * BLOCK_STRIDE;
            if converter == Converter::Adc && self.config.paired_adc {
                // The even block of a pair takes the even samples' phases, the odd one the odd.
                self.regs
                    .write32(at + ADC_NCO_PHASE_MODE, 1 + (b as u32 & 1))?;
            }
            self.regs
                .write32(at + NCO_FREQ_LOWER, frequency as u32 & 0xFFFF)?;
            self.regs
                .write32(at + NCO_FREQ_MIDDLE, (frequency >> 16) as u32 & 0xFFFF)?;
            self.regs
                .write32(at + NCO_FREQ_UPPER, (frequency >> 32) as u32 & 0xFFFF)?;
            self.regs.write32(at + NCO_PHASE_LOWER, phase & 0xFFFF)?;
            self.regs.write32(at + NCO_PHASE_UPPER, phase >> 16)?;
            self.regs
                .modify32(at + NCO_UPDATE_MODE, |v| (v & !UPDATE_SOURCE_MASK) | source)?;
            if source == UPDATE_IMMEDIATE {
                let update = match converter {
                    Converter::Adc => ADC_UPDATE,
                    Converter::Dac => DAC_UPDATE,
                };
                self.regs
                    .modify32(at + update, |v| (v & !0xF) | UPDATE_NCO)?;
            }
        }
        if source == UPDATE_TILE {
            self.regs
                .write32(Self::blocks(converter, tile) + TILE_UPDATE, 1)?;
        }
        Ok(())
    }

    /// A block's mixer setting as its registers hold it.
    pub fn mixer(
        &self,
        converter: Converter,
        tile: usize,
        block: usize,
    ) -> Result<Mixer, RfdcError> {
        let config = self.mixer_block(converter, tile, block)?;
        let rate = self.mixer_rate(converter, tile, block, config)?;
        let b = self.register_blocks(converter, block).start;
        let at = Self::blocks(converter, tile) + b * BLOCK_STRIDE;
        let half = |offset: usize| -> Result<u64, MmioError> {
            Ok(u64::from(self.regs.read32(at + offset)? & 0xFFFF))
        };
        let frequency =
            half(NCO_FREQ_UPPER)? << 32 | half(NCO_FREQ_MIDDLE)? << 16 | half(NCO_FREQ_LOWER)?;
        let phase = (half(NCO_PHASE_UPPER)? << 16 | half(NCO_PHASE_LOWER)?) as u32 & 0x3_FFFF;
        Ok(Mixer {
            frequency_hz: word_frequency(frequency, rate),
            phase_degrees: word_phase(phase),
        })
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend, MockRegion};

    const BASE: u64 = 0xB00C_0000;
    const RATE: u64 = 4_915_200_000;

    /// The RFSoC 4x2 base overlay's converter.
    fn config() -> RfdcConfig {
        let tile = |blocks| TileConfig {
            sample_rate_hz: RATE,
            blocks,
            fine_mixers: blocks,
        };
        RfdcConfig {
            adc: [Some(tile(0b11)), None, Some(tile(0b11)), None],
            dac: [Some(tile(0b1)), None, Some(tile(0b1)), None],
            paired_adc: true,
        }
    }

    /// A converter whose tiles all report that they run.
    fn running() -> (MockBackend, Rfdc<MockRegion>) {
        let be = MockBackend::new();
        for control in [0x4000, 0xC000, 0x14000, 0x1C000] {
            be.poke(BASE + control + 0x0C, 15);
            be.poke(BASE + control + 0x228, 0xC);
        }
        let rf = Rfdc::new(be.map(BASE, 0x40000).unwrap(), config());
        (be, rf)
    }

    fn mixer(frequency_hz: i64) -> Mixer {
        Mixer {
            frequency_hz,
            phase_degrees: 0.0,
        }
    }

    #[test]
    fn frequency_words_are_fractions_of_the_sample_rate() {
        // fs/16 and fs/8, the words the JTAG probe wrote on the board.
        assert_eq!(frequency_word(307_200_000, RATE), 0x1000_0000_0000);
        assert_eq!(frequency_word(614_400_000, RATE), 0x2000_0000_0000);
        assert_eq!(frequency_word(-1_228_800_000, RATE), 0xC000_0000_0000);
        assert_eq!(frequency_word(0, RATE), 0);
        // Rounded toward zero either way: 100 MHz is 5726623061.33 steps.
        assert_eq!(frequency_word(100_000_000, RATE), 5_726_623_061_333);
        assert_eq!(
            frequency_word(-100_000_000, RATE),
            (1 << 48) - 5_726_623_061_333
        );
        for hz in [
            0,
            1,
            -1,
            75_000,
            100_000_000,
            -100_000_000,
            2_457_599_999,
            -2_457_600_000,
        ] {
            assert_eq!(word_frequency(frequency_word(hz, RATE), RATE), hz, "{hz}");
        }
    }

    #[test]
    fn phase_words_cover_the_circle() {
        assert_eq!(phase_word(0.0), 0);
        assert_eq!(phase_word(90.0), 1 << 16);
        assert_eq!(phase_word(-90.0), 0x3_0000);
        for degrees in [0.0, 45.0, -45.0, 179.5, -179.5] {
            assert!((word_phase(phase_word(degrees)) - degrees).abs() < 0.002);
        }
    }

    #[test]
    fn a_dac_block_is_written_and_updated_at_once() {
        let (be, mut rf) = running();
        // DAC tile 2 block 0 is at 0xE000; the update mode starts as another one.
        be.poke(BASE + 0xE08C, 0x12);
        rf.set_mixer(
            Converter::Dac,
            2,
            0,
            Mixer {
                frequency_hz: 614_400_000,
                phase_degrees: 90.0,
            },
        )
        .unwrap();
        assert_eq!(be.peek(BASE + 0xE094), 0x2000);
        assert_eq!(be.peek(BASE + 0xE098), 0);
        assert_eq!(be.peek(BASE + 0xE09C), 0);
        assert_eq!(be.peek(BASE + 0xE0A0), 1);
        assert_eq!(be.peek(BASE + 0xE0A4), 0);
        assert_eq!(be.peek(BASE + 0xE08C), 0x10, "update source: immediate");
        assert_eq!(be.peek(BASE + 0xE020), 2, "update event: oscillator");
        // Nothing of the paired-ADC kind.
        assert_eq!(be.peek(BASE + 0xE0A8), 0);
        assert_eq!(be.peek(BASE + 0xFCB8), 0);
        assert_eq!(
            rf.mixer(Converter::Dac, 2, 0).unwrap(),
            Mixer {
                frequency_hz: 614_400_000,
                phase_degrees: 90.0
            }
        );
    }

    #[test]
    fn a_paired_adc_block_is_two_blocks_updated_with_their_tile() {
        let (be, mut rf) = running();
        // ADC tile 2 starts at 0x1E000; block 1 is its third and fourth blocks of registers.
        rf.set_mixer(Converter::Adc, 2, 1, mixer(-1_228_800_000))
            .unwrap();
        for (at, phase_mode) in [(0x1E800, 1), (0x1EC00, 2)] {
            assert_eq!(be.peek(BASE + at + 0x94), 0xC000);
            assert_eq!(be.peek(BASE + at + 0xA8), phase_mode);
            assert_eq!(be.peek(BASE + at + 0x8C), 2, "update source: tile");
            assert_eq!(be.peek(BASE + at + 0x1C), 0);
        }
        assert_eq!(be.peek(BASE + 0x1FCB8), 1, "tile update");
        for other in [0x1E000, 0x1E400] {
            assert_eq!(be.peek(BASE + other + 0x94), 0, "block 0 untouched");
        }
        // The update comes after the last register of both blocks.
        let writes: Vec<u64> = be
            .log()
            .iter()
            .filter_map(|a| match a {
                hardroxide_mmio::MockAccess::Write { addr, .. } => Some(*addr - BASE),
                _ => None,
            })
            .collect();
        assert_eq!(writes.last(), Some(&0x1FCB8));
        assert_eq!(
            rf.mixer(Converter::Adc, 2, 1).unwrap(),
            mixer(-1_228_800_000)
        );
    }

    #[test]
    fn a_half_rate_dac_halves_the_oscillator_rate_and_a_bypass_is_refused() {
        let (be, mut rf) = running();
        be.poke(BASE + 0x6034, 2);
        rf.set_mixer(Converter::Dac, 0, 0, mixer(614_400_000))
            .unwrap();
        assert_eq!(be.peek(BASE + 0x6094), 0x4000, "a quarter of 2457.6 MS/s");
        assert!(matches!(
            rf.set_mixer(Converter::Dac, 0, 0, mixer(1_300_000_000)),
            Err(RfdcError::Frequency {
                limit: 1_228_800_000,
                ..
            })
        ));
        be.poke(BASE + 0x6034, 3);
        assert!(matches!(
            rf.set_mixer(Converter::Dac, 0, 0, mixer(0)),
            Err(RfdcError::Bypassed(0, 0))
        ));
    }

    #[test]
    fn what_the_design_or_the_tile_cannot_do_is_refused_without_a_write() {
        let (be, mut rf) = running();
        let mut refused = |converter, tile, block, m| {
            let before = be.log().len();
            let error = rf.set_mixer(converter, tile, block, m).unwrap_err();
            assert!(
                !be.log()[before..]
                    .iter()
                    .any(|a| matches!(a, hardroxide_mmio::MockAccess::Write { .. })),
                "{error}"
            );
            error
        };
        assert!(matches!(
            refused(Converter::Adc, 1, 0, mixer(0)),
            RfdcError::NoTile(Converter::Adc, 1)
        ));
        assert!(matches!(
            refused(Converter::Dac, 7, 0, mixer(0)),
            RfdcError::NoTile(..)
        ));
        assert!(matches!(
            refused(Converter::Dac, 0, 1, mixer(0)),
            RfdcError::NoBlock(Converter::Dac, 0, 1)
        ));
        assert!(matches!(
            refused(Converter::Adc, 0, 2, mixer(0)),
            RfdcError::NoBlock(..)
        ));
        for hz in [2_457_600_001, -2_457_600_001, i64::MIN] {
            assert!(matches!(
                refused(Converter::Adc, 0, 0, mixer(hz)),
                RfdcError::Frequency { .. }
            ));
        }
        for phase_degrees in [180.0, -180.0, f32::NAN] {
            let m = Mixer {
                frequency_hz: 0,
                phase_degrees,
            };
            assert!(matches!(
                refused(Converter::Adc, 0, 0, m),
                RfdcError::Phase(_)
            ));
        }
        be.poke(BASE + 0x1400C, 6);
        assert!(matches!(
            refused(Converter::Adc, 0, 0, mixer(0)),
            RfdcError::NotRunning(Converter::Adc, 0, 6)
        ));
    }

    #[test]
    fn a_coarse_mixer_is_not_set() {
        let mut c = config();
        c.adc[0] = Some(TileConfig {
            sample_rate_hz: RATE,
            blocks: 0b11,
            fine_mixers: 0b10,
        });
        let be = MockBackend::new();
        let mut rf = Rfdc::new(be.map(BASE, 0x40000).unwrap(), c);
        assert!(matches!(
            rf.set_mixer(Converter::Adc, 0, 0, mixer(0)),
            Err(RfdcError::NoFineMixer(Converter::Adc, 0, 0))
        ));
    }

    #[test]
    fn starting_a_tile_restarts_it_through_all_its_states() {
        let (be, mut rf) = running();
        be.set_write1_clear(BASE + 0xC004); // the restart bit clears itself
        be.poke(BASE + 0xC008, 0xABCD_0000);
        let status = rf.start_tile(Converter::Dac, 2).unwrap();
        assert!(status.running() && status.pll_locked && status.powered);
        assert_eq!(be.peek(BASE + 0xC008), 0xABCD_010F);
    }

    #[test]
    fn a_tile_without_clocks_times_out_and_says_why() {
        let be = MockBackend::new();
        be.poke(BASE + 0x1400C, 6);
        let mut rf = Rfdc::new(be.map(BASE, 0x40000).unwrap(), config());
        rf.set_timeout(Duration::from_millis(5));
        let error = rf.start_tile(Converter::Adc, 0).unwrap_err();
        let text = error.to_string();
        assert!(
            text.contains("ADC tile 0 did not start: state 6 of 15, PLL not locked")
                && text.contains("clock chips"),
            "{text}"
        );
    }
}
