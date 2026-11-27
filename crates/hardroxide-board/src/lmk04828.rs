//! The TI LMK04828 clock jitter cleaner: SPI mode 0, 24-bit writes (13-bit address, 8-bit value), programmed from a
//! TICS Pro register file ([`crate::tics`]), which normally begins by resetting the chip.
//!
//! **Reset it with [`reset`], not its pin.** The pin is an input only until a register file says otherwise: register
//! `0x14A` can make it an output (the RFSoC 4x2's file makes it an open-drain output held low), and driving it high
//! then pits two outputs against each other. Nor may it float where something pulls it up, which holds the chip in
//! reset. Where a processor pin is wired to it, drive that pin low and leave it there.

use embedded_hal::spi::SpiDevice;

/// Register 0 with `RESET` (self-clearing) and `SPI_3WIRE_DIS` set.
const RESET: u32 = 0x00_0090;

/// Return every register to its power-on value.
pub fn reset<S: SpiDevice>(spi: &mut S) -> Result<(), S::Error> {
    write(spi, RESET)
}

/// Write one 24-bit word.
pub fn write<S: SpiDevice>(spi: &mut S, word: u32) -> Result<(), S::Error> {
    let [_, a, b, c] = word.to_be_bytes();
    spi.write(&[a, b, c])
}

/// Write `words` in order.
pub fn program<S: SpiDevice>(spi: &mut S, words: &[u32]) -> Result<(), S::Error> {
    words.iter().try_for_each(|&w| write(spi, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockSpi;

    #[test]
    fn each_word_is_one_transaction_of_three_bytes() {
        let mut spi = MockSpi::default();
        program(&mut spi, &[0x000010, 0x1555AB]).unwrap();
        reset(&mut spi).unwrap();
        assert_eq!(
            spi.sent,
            [
                vec![0x00, 0x00, 0x10],
                vec![0x15, 0x55, 0xAB],
                vec![0x00, 0x00, 0x90]
            ]
        );
    }
}
