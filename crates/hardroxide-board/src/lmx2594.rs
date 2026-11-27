//! The TI LMX2594 RF synthesizer: SPI mode 0, 24-bit transfers (a read bit, 7-bit address, 16-bit value), programmed
//! from a TICS Pro register file ([`crate::tics`]).
//!
//! The chip's `MUXout` pin either shows lock detect (where boards put an LED) or returns register reads, as bit 2 of
//! register 0 says. [`lock`] switches to reading, asks the chip whether its oscillator is locked, and switches back.

use core::time::Duration;
use embedded_hal::spi::{Operation, SpiDevice};
use hardroxide_mmio::time;

// Register 0.
const RESET: u32 = 1 << 1;
const MUXOUT_LOCK_DETECT: u32 = 1 << 2;
const CALIBRATE: u32 = 1 << 3;
const READ: u8 = 0x80;
/// The register holding `rb_LD_VTUNE` in bits 10:9.
const LOCK_REGISTER: u8 = 110;
/// Registers read back to show the chip answers: the PLL's divider (36), the output multiplexer and power (45) and
/// the channel divider (75). All are plain read/write registers.
const CHECK_REGISTERS: [u8; 3] = [36, 45, 75];

/// What the oscillator's tuning voltage says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    Locked,
    /// Not locked, with the tuning voltage at its low end.
    UnlockedLow,
    /// Not locked, with the tuning voltage at its high end.
    UnlockedHigh,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error<E> {
    #[error("SPI: {0:?}")]
    Spi(E),
    #[error("the register values hold no register 0, or no register to check reading with")]
    Incomplete,
    #[error(
        "the chip does not read back: register {register} gave {read:#06x}, not the {written:#06x} written"
    )]
    NoReadback {
        register: u8,
        written: u16,
        read: u16,
    },
}

fn write<S: SpiDevice>(spi: &mut S, word: u32) -> Result<(), S::Error> {
    let [_, a, b, c] = word.to_be_bytes();
    spi.write(&[a, b, c])
}

/// The last value `words` write to `register`.
fn written(words: &[u32], register: u8) -> Option<u16> {
    words
        .iter()
        .rev()
        .find(|&&w| (w >> 16) as u8 == register)
        .map(|&w| w as u16)
}

/// Reset the chip, write `words` in order, wait for the oscillator's supplies and write register 0 again to calibrate
/// it. Takes about 20 ms.
pub fn program<S: SpiDevice>(spi: &mut S, words: &[u32]) -> Result<(), Error<S::Error>> {
    let r0 = written(words, 0).ok_or(Error::Incomplete)?;
    write(spi, RESET).map_err(Error::Spi)?;
    write(spi, 0).map_err(Error::Spi)?;
    for &w in words {
        write(spi, w).map_err(Error::Spi)?;
    }
    time::sleep(Duration::from_millis(20));
    write(spi, u32::from(r0)).map_err(Error::Spi)
}

/// Read `register`. The chip answers only while `MUXout` is in read mode.
pub fn read_register<S: SpiDevice>(spi: &mut S, register: u8) -> Result<u16, S::Error> {
    let mut value = [0; 2];
    spi.transaction(&mut [
        Operation::Write(&[READ | register]),
        Operation::Read(&mut value),
    ])?;
    Ok(u16::from_be_bytes(value))
}

/// Whether the chip, programmed with `words`, is locked. It must first read back registers 36, 45 and 75 (those
/// `words` set) as written, so that a missing or silent chip cannot pass. `MUXout` shows lock detect again afterwards, if `words` set it so.
pub fn lock<S: SpiDevice>(spi: &mut S, words: &[u32]) -> Result<Lock, Error<S::Error>> {
    let r0 = u32::from(written(words, 0).ok_or(Error::Incomplete)?) & !CALIBRATE;
    let checks = CHECK_REGISTERS.map(|r| written(words, r).map(|v| (r, v)));
    // A value of all zeros or all ones proves nothing: a bus stuck either way reads it.
    if !checks.iter().flatten().any(|&(_, v)| v != 0 && v != 0xFFFF) {
        return Err(Error::Incomplete);
    }
    write(spi, r0 & !MUXOUT_LOCK_DETECT).map_err(Error::Spi)?;
    let result = (|| {
        for (register, expected) in checks.into_iter().flatten() {
            let read = read_register(spi, register).map_err(Error::Spi)?;
            if read != expected {
                return Err(Error::NoReadback {
                    register,
                    written: expected,
                    read,
                });
            }
        }
        let status = read_register(spi, LOCK_REGISTER).map_err(Error::Spi)?;
        Ok(match (status >> 9) & 3 {
            2 => Lock::Locked,
            3 => Lock::UnlockedHigh,
            _ => Lock::UnlockedLow,
        })
    })();
    write(spi, r0).map_err(Error::Spi)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockSpi;

    // Register 112, a check register (36), and register 0 with lock detect and calibration on.
    const WORDS: [u32; 3] = [0x70_0000, 0x24_0046, 0x00_241C];

    #[test]
    fn programming_resets_writes_and_calibrates() {
        let mut spi = MockSpi::default();
        program(&mut spi, &WORDS).unwrap();
        assert_eq!(
            spi.sent,
            [
                vec![0, 0, 2],
                vec![0, 0, 0],
                vec![0x70, 0, 0],
                vec![0x24, 0, 0x46],
                vec![0, 0x24, 0x1C],
                vec![0, 0x24, 0x1C]
            ]
        );
        assert_eq!(program(&mut spi, &WORDS[..2]), Err(Error::Incomplete));
    }

    #[test]
    fn lock_reads_back_first_and_restores_lock_detect() {
        let mut spi = MockSpi::default();
        spi.answers.extend([vec![0x00, 0x46], vec![0x04, 0x00]]);
        assert_eq!(lock(&mut spi, &WORDS), Ok(Lock::Locked));
        assert_eq!(
            spi.sent,
            [
                // Register 0 without calibration (bit 3), with MUXout reading (bit 2 off)...
                vec![0, 0x24, 0x10],
                vec![0x80 | 36],
                vec![0x80 | 110],
                // ...and showing lock detect again.
                vec![0, 0x24, 0x14]
            ]
        );
        spi.answers.extend([vec![0x00, 0x46], vec![0x06, 0x00]]);
        assert_eq!(lock(&mut spi, &WORDS), Ok(Lock::UnlockedHigh));
    }

    #[test]
    fn a_silent_chip_is_not_locked() {
        let mut spi = MockSpi::default();
        spi.answers.push_back(vec![0xFF, 0xFF]);
        assert_eq!(
            lock(&mut spi, &WORDS),
            Err(Error::NoReadback {
                register: 36,
                written: 0x46,
                read: 0xFFFF
            })
        );
        assert_eq!(spi.sent.last().unwrap(), &[0, 0x24, 0x14]);
    }
}
