//! The Real Digital RFSoC 4x2: which processor pins and buses reach its chips (Reference Manual revision A6 and the
//! board's device tree).

/// The processor's two LEDs (`PS_LED0`, `PS_LED1`), as GPIO (MIO) pins. Lit when high.
pub const PS_LEDS: [u32; 2] = [20, 17];
/// The processor's push button (`PS_BTN`). High while pressed.
pub const PS_BUTTON: u32 = 23;

/// The I2C controller of the power monitors.
pub const MONITOR_I2C: usize = 0;
/// A supply with an INA220 monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rail {
    pub name: &'static str,
    /// The monitor's I2C address.
    pub address: u8,
    /// The shunt resistor, in millionths of an ohm.
    pub shunt_micro_ohms: u32,
}

const fn rail(name: &'static str, address: u8, shunt_micro_ohms: u32) -> Rail {
    Rail {
        name,
        address,
        shunt_micro_ohms,
    }
}

/// The monitored supplies.
pub const RAILS: [Rail; 9] = [
    rail("VCC0V85", 0x40, 1000),
    rail("VCC3V3", 0x41, 1000),
    rail("VCC1V8", 0x42, 1000),
    rail("VDAC_AVCC", 0x43, 10_000),
    rail("VADC_AVCC", 0x44, 10_000),
    rail("VADC_AVCCAUX", 0x45, 10_000),
    rail("VDAC_AVCCAUX", 0x46, 10_000),
    rail("VDAC_AVTT", 0x47, 10_000),
    rail("SYZYGY_VIO", 0x48, 10_000),
];

/// The I2C controller of the identity EEPROM ([`crate::at24mac`]).
pub const IDENTITY_I2C: usize = 1;

/// The SPI controller of the OLED display ([`crate::us2066`], mode 3) and its chip select.
pub const OLED_SPI: usize = 1;
pub const OLED_CHIP_SELECT: u8 = 0;
/// The SPI controller of the clock chips (mode 0).
pub const CLOCK_SPI: usize = 0;
/// Chip select of the LMK04828.
pub const LMK_CHIP_SELECT: u8 = 0;
/// Chip select of the LMX2594 feeding the DAC tiles.
pub const LMX_DAC_CHIP_SELECT: u8 = 1;
/// Chip select of the LMX2594 feeding the ADC tiles.
pub const LMX_ADC_CHIP_SELECT: u8 = 2;
/// The fastest SPI clock to use for the display and the clock chips.
pub const SPI_MAX_HZ: u32 = 1_000_000;
/// The GPIO pin wired to the LMK04828's reset pin. **Drive it low, before anything is sent to the chip, and never
/// high.** Left as an input, the processor pin's pull-up holds the chip in reset once the chip's pin is an input; the
/// board's register file then turns the chip's pin into an output held low, which a high level here would fight. Reset
/// the chip with [`crate::lmk04828::reset`] instead.
pub const LMK_RESET: u32 = 7;
/// The LMK04828's input clock select pins (GPIO); both low selects the board's oscillator.
pub const LMK_CLOCK_SELECT: [u32; 2] = [8, 12];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rails_have_their_own_addresses() {
        for (i, a) in RAILS.iter().enumerate() {
            assert_eq!(a.address, 0x40 + i as u8);
        }
    }
}
