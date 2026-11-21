//! The Analog Devices ADAU1761 codec on the PYNQ-Z2, configured over I2C.
//!
//! Each register write is three bytes: the register address (a 16-bit address whose high byte is
//! `0x40` for the control registers) and the value; a PLL write carries six data bytes at once. The
//! initialisation sequence (an input master clock of 10 MHz into the PLL to make 49.152 MHz, the
//! converters, mixers and serial port) follows PYNQ's audio driver (Apache-2.0), with the PLL
//! values worked out in the comments below.

use embedded_hal::i2c::I2c;

/// The codec's 7-bit I2C address with both address pins high, as the overlay wires them.
pub const ADDRESS: u8 = 0x3B;

/// Register addresses (the low byte; the high byte is `0x40`), named as in the data sheet.
pub mod reg {
    pub const CLOCK_CONTROL: u8 = 0x00;
    pub const PLL_CONTROL: u8 = 0x02;
    pub const RECORD_MIXER_LEFT_0: u8 = 0x0A;
    pub const RECORD_MIXER_LEFT_1: u8 = 0x0B;
    pub const RECORD_MIXER_RIGHT_0: u8 = 0x0C;
    pub const RECORD_MIXER_RIGHT_1: u8 = 0x0D;
    pub const LEFT_DIFF_INPUT_VOLUME: u8 = 0x0E;
    pub const RIGHT_DIFF_INPUT_VOLUME: u8 = 0x0F;
    pub const MIC_BIAS: u8 = 0x10;
    pub const ALC_CONTROL_3: u8 = 0x14;
    pub const SERIAL_PORT_0: u8 = 0x15;
    pub const ADC_CONTROL: u8 = 0x19;
    pub const PLAYBACK_MIXER_LEFT_0: u8 = 0x1C;
    pub const PLAYBACK_MIXER_LEFT_1: u8 = 0x1D;
    pub const PLAYBACK_MIXER_RIGHT_0: u8 = 0x1E;
    pub const PLAYBACK_MIXER_RIGHT_1: u8 = 0x1F;
    pub const HEADPHONE_LEFT_VOLUME: u8 = 0x23;
    pub const HEADPHONE_RIGHT_VOLUME: u8 = 0x24;
    pub const PLAYBACK_POWER: u8 = 0x29;
    pub const DAC_CONTROL_0: u8 = 0x2A;
    pub const SERIAL_INPUT_ROUTE: u8 = 0xF2;
    pub const SERIAL_OUTPUT_ROUTE: u8 = 0xF3;
    pub const DSP_ENABLE: u8 = 0xF5;
    pub const DSP_RUN: u8 = 0xF6;
    pub const CLOCK_ENABLE_0: u8 = 0xF9;
    pub const CLOCK_ENABLE_1: u8 = 0xFA;
}

/// Where recorded audio comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    LineIn,
    Microphone,
    None,
}

pub struct Adau1761<I: I2c> {
    i2c: I,
}

impl<I: I2c> Adau1761<I> {
    pub fn new(i2c: I) -> Self {
        Adau1761 { i2c }
    }

    pub fn release(self) -> I {
        self.i2c
    }

    /// Write one control register.
    pub fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), I::Error> {
        self.i2c.write(ADDRESS, &[0x40, reg, value])
    }

    /// Read `buf.len()` registers starting at `reg` (a write of the address, then a read).
    pub fn read_regs(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), I::Error> {
        self.i2c.write(ADDRESS, &[0x40, reg])?;
        self.i2c.read(ADDRESS, buf)
    }

    /// Set up the clock PLL: from the 10 MHz master clock the board supplies to 1024 x 48 kHz
    /// = 49.152 MHz. R + N/M = 49.152 / 10 = 4.9152 = 4 + 572/625. Returns `Ok(false)` if the PLL
    /// did not report a lock within `polls` reads.
    pub fn start_pll(&mut self, polls: u32) -> Result<bool, I::Error> {
        // Core clock off while the PLL is changed.
        self.write_reg(reg::CLOCK_CONTROL, 0x0E)?;
        // M = 0x0271 = 625, N = 0x023C = 572, then R = 4, X = 0 (integer 0), fractional mode on, and enable.
        self.i2c.write(
            ADDRESS,
            &[0x40, reg::PLL_CONTROL, 0x02, 0x71, 0x02, 0x3C, 0x21, 0x03],
        )?;
        let mut locked = false;
        for _ in 0..polls {
            let mut r = [0u8; 6];
            self.read_regs(reg::PLL_CONTROL, &mut r)?;
            if r[5] & 0x02 != 0 {
                locked = true;
                break;
            }
        }
        if locked {
            // Core clock from the PLL, 1024 x fs input frequency setting, core enabled.
            self.write_reg(reg::CLOCK_CONTROL, 0x0F)?;
        }
        Ok(locked)
    }

    /// The converter, mixer and serial-port setup: the codec is the I2S slave, ADCs and DACs on,
    /// inputs muted, outputs muted at the headphone amplifier until [`Adau1761::enable_playback`].
    pub fn configure(&mut self) -> Result<(), I::Error> {
        use reg::*;
        for (r, v) in [
            (RECORD_MIXER_LEFT_0, 0x00),
            (RECORD_MIXER_RIGHT_0, 0x00),
            (LEFT_DIFF_INPUT_VOLUME, 0xB3),
            (RIGHT_DIFF_INPUT_VOLUME, 0xB3),
            (MIC_BIAS, 0x01),
            (ALC_CONTROL_3, 0x20),
            (SERIAL_PORT_0, 0x00),
            (ADC_CONTROL, 0x33),
            (PLAYBACK_MIXER_LEFT_0, 0x00),
            (PLAYBACK_MIXER_RIGHT_0, 0x00),
            (PLAYBACK_MIXER_LEFT_1, 0x00),
            (PLAYBACK_MIXER_RIGHT_1, 0x00),
            (HEADPHONE_LEFT_VOLUME, 0xE5),
            (HEADPHONE_RIGHT_VOLUME, 0xE5),
            (PLAYBACK_POWER, 0x03),
            (DAC_CONTROL_0, 0x03),
            (SERIAL_INPUT_ROUTE, 0x01),
            (SERIAL_OUTPUT_ROUTE, 0x01),
            (DSP_ENABLE, 0x01),
            (DSP_RUN, 0x01),
            // Clock generators 0 (converters and DSP) and 1 (the serial port's bit and frame clocks).
            (CLOCK_ENABLE_0, 0x7F),
            (CLOCK_ENABLE_1, 0x03),
        ] {
            self.write_reg(r, v)?;
        }
        Ok(())
    }

    /// The whole bring-up: PLL, then configuration. `Ok(false)` if the PLL did not lock.
    pub fn init(&mut self) -> Result<bool, I::Error> {
        if !self.start_pll(1000)? {
            return Ok(false);
        }
        self.configure()?;
        Ok(true)
    }

    /// Choose the record input.
    pub fn select_input(&mut self, input: Input) -> Result<(), I::Error> {
        use reg::*;
        match input {
            Input::LineIn => {
                self.write_reg(RECORD_MIXER_LEFT_0, 0x01)?;
                self.write_reg(RECORD_MIXER_LEFT_1, 0x07)?;
                self.write_reg(RECORD_MIXER_RIGHT_0, 0x01)?;
                self.write_reg(RECORD_MIXER_RIGHT_1, 0x07)
            }
            Input::Microphone => {
                self.write_reg(RECORD_MIXER_LEFT_0, 0x01)?;
                self.write_reg(RECORD_MIXER_LEFT_1, 0x10)?;
                self.write_reg(LEFT_DIFF_INPUT_VOLUME, 0xB3)?;
                self.write_reg(RECORD_MIXER_RIGHT_0, 0x01)?;
                self.write_reg(RECORD_MIXER_RIGHT_1, 0x10)?;
                self.write_reg(RIGHT_DIFF_INPUT_VOLUME, 0xB3)
            }
            Input::None => {
                self.write_reg(RECORD_MIXER_LEFT_0, 0x00)?;
                self.write_reg(RECORD_MIXER_RIGHT_0, 0x00)
            }
        }
    }

    /// Unmute the DACs into the headphone amplifier at `volume` (0 = -57 dB to 62 = +5 dB).
    pub fn enable_playback(&mut self, volume: u8) -> Result<(), I::Error> {
        use reg::*;
        self.write_reg(PLAYBACK_MIXER_LEFT_0, 0x21)?;
        self.write_reg(PLAYBACK_MIXER_RIGHT_0, 0x41)?;
        self.set_volume(volume)
    }

    /// Headphone volume: 0 = -57 dB up to 62 = +5 dB in 1 dB steps (63 is reserved).
    pub fn set_volume(&mut self, volume: u8) -> Result<(), I::Error> {
        let v = volume.min(62) << 2 | 0x3; // 0b11: unmute and enable the amplifier
        self.write_reg(reg::HEADPHONE_LEFT_VOLUME, v)?;
        self.write_reg(reg::HEADPHONE_RIGHT_VOLUME, v)
    }

    /// Stop playback: DAC mixers muted, amplifier muted.
    pub fn mute(&mut self) -> Result<(), I::Error> {
        use reg::*;
        self.write_reg(PLAYBACK_MIXER_LEFT_0, 0x01)?;
        self.write_reg(PLAYBACK_MIXER_RIGHT_0, 0x01)?;
        self.write_reg(HEADPHONE_LEFT_VOLUME, 0xE5)?;
        self.write_reg(HEADPHONE_RIGHT_VOLUME, 0xE5)
    }

    /// Route the ADC straight to the output mixers (the codec-side pass-through).
    pub fn enable_bypass(&mut self, volume: u8) -> Result<(), I::Error> {
        use reg::*;
        self.write_reg(PLAYBACK_MIXER_LEFT_1, 0x00)?;
        self.write_reg(PLAYBACK_MIXER_RIGHT_1, 0x00)?;
        self.write_reg(PLAYBACK_MIXER_LEFT_0, 0x21)?;
        self.write_reg(PLAYBACK_MIXER_RIGHT_0, 0x41)?;
        self.set_volume(volume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    /// An I2C bus that records transactions and answers reads from a script.
    #[derive(Default)]
    struct Bus {
        writes: Vec<Vec<u8>>,
        read_data: Vec<Vec<u8>>,
    }
    impl ErrorType for Bus {
        type Error = ErrorKind;
    }
    impl I2c for Bus {
        fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
            assert_eq!(address, ADDRESS);
            for op in ops {
                match op {
                    Operation::Write(d) => self.writes.push(d.to_vec()),
                    Operation::Read(b) => {
                        let r = if self.read_data.is_empty() {
                            Vec::new()
                        } else {
                            self.read_data.remove(0)
                        };
                        let n = b.len().min(r.len());
                        b[..n].copy_from_slice(&r[..n]);
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn a_register_write_is_address_high_address_low_value() {
        let mut c = Adau1761::new(Bus::default());
        c.write_reg(reg::ADC_CONTROL, 0x33).unwrap();
        assert_eq!(c.release().writes, [[0x40, 0x19, 0x33]]);
    }

    #[test]
    fn the_pll_is_set_for_49_152_mhz_from_10_mhz() {
        let mut bus = Bus::default();
        bus.read_data.push([0, 0, 0, 0, 0, 0x02].to_vec()); // locked at once
        let mut c = Adau1761::new(bus);
        assert!(c.start_pll(10).unwrap());
        let w = c.release().writes;
        assert_eq!(w[0], [0x40, 0x00, 0x0E]); // core clock off
        // M = 625 = 0x0271, N = 572 = 0x023C, then the R/X/mode byte and the enable byte.
        assert_eq!(w[1], [0x40, 0x02, 0x02, 0x71, 0x02, 0x3C, 0x21, 0x03]);
        assert_eq!(w[2], [0x40, 0x02]); // the lock poll's address write
        assert_eq!(*w.last().unwrap(), [0x40, 0x00, 0x0F]); // core clock from the PLL
        // 10 MHz x (4 + 572/625) = 49.152 MHz.
        assert!((10.0e6 * (4.0 + 572.0 / 625.0) - 49.152e6f64).abs() < 1.0);
    }

    #[test]
    fn a_pll_that_never_locks_is_reported_and_the_core_clock_stays_off() {
        let mut c = Adau1761::new(Bus::default());
        assert!(!c.start_pll(3).unwrap());
        assert!(!c.release().writes.iter().any(|w| w == &[0x40, 0x00, 0x0F]));
    }

    #[test]
    fn volume_maps_to_the_register_with_the_unmute_bits() {
        let mut c = Adau1761::new(Bus::default());
        c.set_volume(57).unwrap();
        c.set_volume(200).unwrap(); // clamped to 62
        let w = c.release().writes;
        assert_eq!(w[0], [0x40, 0x23, 57 << 2 | 3]);
        assert_eq!(w[1], [0x40, 0x24, 57 << 2 | 3]);
        assert_eq!(w[2][2], 62 << 2 | 3);
    }

    #[test]
    fn configure_writes_the_converters_and_enables_the_clocks_last() {
        let mut c = Adau1761::new(Bus::default());
        c.configure().unwrap();
        let w = c.release().writes;
        assert!(w.iter().any(|x| x == &[0x40, 0x19, 0x33]));
        assert!(w.iter().any(|x| x == &[0x40, 0xF2, 0x01]));
        assert_eq!(w[w.len() - 2], [0x40, 0xF9, 0x7F]);
        assert_eq!(w[w.len() - 1], [0x40, 0xFA, 0x03]);
        assert!(w.iter().all(|x| x.len() == 3 && x[0] == 0x40));
    }

    #[test]
    fn inputs_and_playback() {
        let mut c = Adau1761::new(Bus::default());
        c.select_input(Input::LineIn).unwrap();
        c.enable_playback(40).unwrap();
        c.select_input(Input::None).unwrap();
        let w = c.release().writes;
        assert_eq!(w[0], [0x40, 0x0A, 0x01]);
        assert_eq!(w[1], [0x40, 0x0B, 0x07]);
        assert!(w.iter().any(|x| x == &[0x40, 0x1C, 0x21]));
        assert_eq!(w[w.len() - 2], [0x40, 0x0A, 0x00]);
    }
}
