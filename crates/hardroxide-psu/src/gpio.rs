//! The processing system's GPIO controller: the 78 multiplexed I/O pins (MIO 0 to 77) and the 96 signals into the
//! programmable logic (EMIO, pins 78 to 173), in six banks.
//!
//! A pin used here must be routed to GPIO by `psu_init`. Writes go through the masked data registers, so changing one pin
//! never disturbs another.

use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

const MASKED_DATA: usize = 0x000; // two registers a bank: pins 0..16 and 16.. ([31:16] mask, 0 = write)
const DATA_IN: usize = 0x060; // one register a bank
const DIRECTION: usize = 0x204; // per bank, 0x40 apart
const OUTPUT_ENABLE: usize = 0x208;

/// The pins in each bank.
const BANKS: [u32; 6] = [26, 26, 26, 32, 32, 32];
/// The number of pins.
pub const PINS: u32 = 174;

/// The bank of `pin` and its bit there.
pub fn locate(pin: u32) -> Option<(usize, u32)> {
    let mut first = 0;
    for (bank, &size) in BANKS.iter().enumerate() {
        if pin < first + size {
            return Some((bank, pin - first));
        }
        first += size;
    }
    None
}

pub struct Gpio<R: Mmio> {
    regs: R,
}

impl<R: Mmio> Gpio<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    fn pin(pin: u32) -> Result<(usize, u32)> {
        locate(pin).ok_or(MmioError::OutOfBounds {
            offset: pin as usize,
            len: PINS as usize,
        })
    }

    /// Make `pin` an output driving `high`, without driving the other level first.
    pub fn set_output(&self, pin: u32, high: bool) -> Result<()> {
        let (bank, bit) = Self::pin(pin)?;
        // The controller ignores a level written while the pin is an input, so: direction first, with the driver still
        // off; then the level; then the driver. (Seen on the board; enabling the driver first drives whatever level
        // the pin last had.)
        self.regs
            .modify32(DIRECTION + 0x40 * bank, |v| v | 1 << bit)?;
        self.write(pin, high)?;
        self.regs
            .modify32(OUTPUT_ENABLE + 0x40 * bank, |v| v | 1 << bit)
    }

    /// Make `pin` an input.
    pub fn set_input(&self, pin: u32) -> Result<()> {
        let (bank, bit) = Self::pin(pin)?;
        self.regs
            .modify32(OUTPUT_ENABLE + 0x40 * bank, |v| v & !(1 << bit))?;
        self.regs
            .modify32(DIRECTION + 0x40 * bank, |v| v & !(1 << bit))
    }

    /// Set the level an output `pin` drives. Has no effect on an input: make it an output with [`Gpio::set_output`].
    pub fn write(&self, pin: u32, high: bool) -> Result<()> {
        let (bank, bit) = Self::pin(pin)?;
        let (half, bit) = (bit / 16, bit % 16);
        // The mask in the upper half is active low: only this pin's bit is written.
        let value = (!(1u32 << bit) & 0xFFFF) << 16 | u32::from(high) << bit;
        self.regs
            .write32(MASKED_DATA + 8 * bank + 4 * half as usize, value)
    }

    /// The level on `pin`.
    pub fn read(&self, pin: u32) -> Result<bool> {
        let (bank, bit) = Self::pin(pin)?;
        Ok(self.regs.read32(DATA_IN + 4 * bank)? & (1 << bit) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockAccess, MockBackend};

    const BASE: u64 = 0xFF0A_0000;

    #[test]
    fn pins_are_found_in_their_banks() {
        assert_eq!(locate(0), Some((0, 0)));
        assert_eq!(locate(25), Some((0, 25)));
        assert_eq!(locate(26), Some((1, 0)));
        assert_eq!(locate(77), Some((2, 25)));
        assert_eq!(locate(78), Some((3, 0)));
        assert_eq!(locate(173), Some((5, 31)));
        assert_eq!(locate(174), None);
    }

    #[test]
    fn writing_a_pin_masks_every_other() {
        let be = MockBackend::new();
        let g = Gpio::new(be.map(BASE, 0x400).unwrap());
        g.write(20, true).unwrap(); // bank 0, upper half, bit 4
        assert_eq!(be.peek(BASE + 0x004), 0xFFEF_0010);
        g.write(23, false).unwrap();
        assert_eq!(be.peek(BASE + 0x004), 0xFF7F_0000);
        g.write(28, true).unwrap(); // bank 1, lower half, bit 2
        assert_eq!(be.peek(BASE + 0x008), 0xFFFB_0004);
        assert!(g.write(174, true).is_err());
    }

    #[test]
    fn direction_and_input() {
        let be = MockBackend::new();
        let g = Gpio::new(be.map(BASE, 0x400).unwrap());
        be.poke(BASE + 0x204, 1 << 3);
        g.set_output(17, false).unwrap();
        // The level goes out between selecting the direction and enabling the driver.
        let writes: std::vec::Vec<u64> = be
            .log()
            .iter()
            .filter_map(|a| match *a {
                MockAccess::Write { addr, .. } => Some(addr - BASE),
                _ => None,
            })
            .collect();
        assert_eq!(writes, [0x204, 0x004, 0x208]);
        assert_eq!(be.peek(BASE + 0x204), 1 << 3 | 1 << 17);
        assert_eq!(be.peek(BASE + 0x208), 1 << 17);
        g.set_input(17).unwrap();
        assert_eq!(be.peek(BASE + 0x204), 1 << 3);
        assert_eq!(be.peek(BASE + 0x208), 0);
        be.poke(BASE + 0x060, 1 << 23);
        assert!(g.read(23).unwrap());
        assert!(!g.read(22).unwrap());
    }
}
