//! The US2066 OLED controller in its serial (SPI) mode, as on the NewHaven NHD-0216AW 16x2 character display. The bus
//! is SPI mode 3, at most 1 MHz. Bytes travel least significant bit first inside three-byte frames: a start byte that
//! says what follows, then each nibble of the value in the upper half of a byte.
//!
//! The initialisation sequence is the one in RFSoC-PYNQ's driver for this display (BSD-3-Clause).

use core::time::Duration;
use embedded_hal::spi::{Operation, SpiDevice};
use hardroxide_mmio::time;

/// Rows of the 16x2 display.
pub const ROWS: usize = 2;
/// Characters in a row.
pub const COLUMNS: usize = 16;

// Start bytes: five ones, then read/write, then data/command.
const WRITE_COMMAND: u8 = 0xF8;
const WRITE_DATA: u8 = 0xFA;
const READ_STATUS: u8 = 0xFC;

/// `(true, value)` is a data byte of the sequence.
const INIT: [(bool, u8); 27] = {
    const C: bool = false;
    const D: bool = true;
    [
        (C, 0x2A),
        (C, 0x71),
        (D, 0x00), // function set: extended; internal regulator off
        (C, 0x28),
        (C, 0x08), // display off
        (C, 0x2A),
        (C, 0x79),
        (C, 0xD5),
        (C, 0x70),
        (C, 0x78), // oscillator
        (C, 0x08),
        (C, 0x06), // two lines; segment and common direction
        (C, 0x72),
        (D, 0x00), // character set A
        (C, 0x2A),
        (C, 0x79),
        (C, 0xDA),
        (C, 0x00), // segment pins
        (C, 0xDC),
        (C, 0x00),
        (C, 0x81),
        (C, 0x7F), // contrast
        (C, 0xD9),
        (C, 0xF1),
        (C, 0xDB),
        (C, 0x40), // timing and voltage levels
        (C, 0x78),
    ]
};
const LEAVE_EXTENDED: u8 = 0x28;
const CLEAR: u8 = 0x01;
const DISPLAY_ON: u8 = 0x0C;
const SET_ADDRESS: u8 = 0x80;
/// The character address at which the second row starts.
const SECOND_ROW: u8 = 0x40;

fn frame(start: u8, value: u8) -> [u8; 3] {
    [
        start,
        (value & 0x0F).reverse_bits(),
        (value >> 4).reverse_bits(),
    ]
}

pub struct Oled<S: SpiDevice> {
    spi: S,
}

impl<S: SpiDevice> Oled<S> {
    pub fn new(spi: S) -> Self {
        Self { spi }
    }

    fn command(&mut self, value: u8) -> Result<(), S::Error> {
        self.spi.write(&frame(WRITE_COMMAND, value))?;
        time::sleep(Duration::from_millis(1));
        Ok(())
    }

    fn data(&mut self, value: u8) -> Result<(), S::Error> {
        self.spi.write(&frame(WRITE_DATA, value))?;
        time::sleep(Duration::from_micros(100));
        Ok(())
    }

    /// Set the display up, blank and switched on. Takes about 0.2 s.
    pub fn init(&mut self) -> Result<(), S::Error> {
        time::sleep(Duration::from_millis(100));
        for (is_data, value) in INIT {
            if is_data {
                self.data(value)?;
            } else {
                self.command(value)?;
            }
        }
        self.command(LEAVE_EXTENDED)?;
        self.clear()?;
        self.command(DISPLAY_ON)?;
        time::sleep(Duration::from_millis(100));
        Ok(())
    }

    /// Blank the display.
    pub fn clear(&mut self) -> Result<(), S::Error> {
        self.command(CLEAR)?;
        time::sleep(Duration::from_millis(5));
        Ok(())
    }

    /// Show `text` on `row` (0 or 1; other values show on the second), padded or cut to [`COLUMNS`]. Characters
    /// outside printable ASCII show as `?`.
    pub fn write_line(&mut self, row: usize, text: &str) -> Result<(), S::Error> {
        let start = if row == 0 { 0 } else { SECOND_ROW };
        self.command(SET_ADDRESS | start)?;
        let mut chars = text.chars();
        for _ in 0..COLUMNS {
            let byte = match chars.next() {
                Some(c @ ' '..='~') => c as u8,
                Some(_) => b'?',
                None => b' ',
            };
            self.data(byte)?;
        }
        Ok(())
    }

    /// The controller's address counter: where the next character goes (`0x40` onwards is the second row). Reading
    /// it shows that the display is there and following. `None` while the controller is busy.
    pub fn address(&mut self) -> Result<Option<u8>, S::Error> {
        let mut status = [0];
        self.spi.transaction(&mut [
            Operation::Write(&[READ_STATUS]),
            Operation::Read(&mut status),
        ])?;
        // Bit 7 is the busy flag.
        let status = status[0].reverse_bits();
        Ok((status & 0x80 == 0).then_some(status))
    }

    /// Move to character address `address` (for [`Oled::address`] to report).
    pub fn set_address(&mut self, address: u8) -> Result<(), S::Error> {
        self.command(SET_ADDRESS | (address & 0x7F))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockSpi;

    #[test]
    fn frames_carry_each_nibble_least_significant_bit_first() {
        // As the vendor's driver does it: 0x2A is nibbles A then 2, each bit-reversed into the upper half.
        assert_eq!(frame(WRITE_COMMAND, 0x2A), [0xF8, 0x50, 0x40]);
        assert_eq!(frame(WRITE_DATA, 0x41), [0xFA, 0x80, 0x20]);
    }

    #[test]
    fn a_line_is_addressed_and_padded() {
        let mut oled = Oled::new(MockSpi::default());
        oled.write_line(1, "Hi\u{b0}").unwrap();
        let sent = &oled.spi.sent;
        assert_eq!(sent.len(), 1 + COLUMNS);
        assert_eq!(sent[0], frame(WRITE_COMMAND, 0xC0));
        assert_eq!(sent[1], frame(WRITE_DATA, b'H'));
        assert_eq!(sent[3], frame(WRITE_DATA, b'?'));
        assert_eq!(sent[16], frame(WRITE_DATA, b' '));
        oled.spi.sent.clear();
        oled.write_line(0, "a line much too long for it").unwrap();
        assert_eq!(oled.spi.sent.len(), 1 + COLUMNS);
        assert_eq!(oled.spi.sent[0], frame(WRITE_COMMAND, 0x80));
    }

    #[test]
    fn initialisation_ends_switched_on_and_the_status_reads_back() {
        let mut oled = Oled::new(MockSpi::default());
        oled.init().unwrap();
        assert_eq!(oled.spi.sent.last().unwrap(), &frame(WRITE_COMMAND, 0x0C));
        assert_eq!(oled.spi.sent.len(), INIT.len() + 3);
        // 0x43 reversed is 0xC2; 0x80 (busy) reversed is 0x01.
        oled.spi.answers.extend([vec![0xC2], vec![0x01]]);
        assert_eq!(oled.address().unwrap(), Some(0x43));
        assert_eq!(oled.address().unwrap(), None);
        assert_eq!(oled.spi.sent.last().unwrap(), &[0xFC]);
    }
}
