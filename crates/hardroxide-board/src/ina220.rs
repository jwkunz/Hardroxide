//! The TI INA220 current, voltage and power monitor. In its reset configuration it measures continuously (bus voltage
//! up to 32 V, shunt voltage up to ±320 mV, 12 bits), so reading needs no setup: the only bytes this writes are
//! register pointers.

use embedded_hal::i2c::I2c;

const CONFIGURATION: u8 = 0x00;
const SHUNT_VOLTAGE: u8 = 0x01;
const BUS_VOLTAGE: u8 = 0x02;

/// The configuration register after a reset.
pub const RESET_CONFIGURATION: u16 = 0x399F;

/// One measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    /// The supply's voltage.
    pub bus_millivolts: u32,
    /// The voltage across the shunt resistor (negative if the current flows the other way).
    pub shunt_microvolts: i32,
}

impl Reading {
    /// The current through a shunt of `shunt_micro_ohms`.
    pub fn milliamps(&self, shunt_micro_ohms: u32) -> i32 {
        (i64::from(self.shunt_microvolts) * 1000 / i64::from(shunt_micro_ohms.max(1))) as i32
    }

    /// The power delivered through a shunt of `shunt_micro_ohms`.
    pub fn milliwatts(&self, shunt_micro_ohms: u32) -> i32 {
        (i64::from(self.milliamps(shunt_micro_ohms)) * i64::from(self.bus_millivolts) / 1000) as i32
    }
}

fn register<I: I2c>(bus: &mut I, address: u8, register: u8) -> Result<u16, I::Error> {
    bus.write(address, &[register])?;
    let mut value = [0; 2];
    bus.read(address, &mut value)?;
    Ok(u16::from_be_bytes(value))
}

/// The configuration register of the monitor at `address`.
pub fn configuration<I: I2c>(bus: &mut I, address: u8) -> Result<u16, I::Error> {
    register(bus, address, CONFIGURATION)
}

/// Read both voltages of the monitor at `address`.
pub fn read<I: I2c>(bus: &mut I, address: u8) -> Result<Reading, I::Error> {
    let shunt = register(bus, address, SHUNT_VOLTAGE)? as i16;
    let supply = register(bus, address, BUS_VOLTAGE)?;
    Ok(Reading {
        // Bits 15 to 3, in steps of 4 mV.
        bus_millivolts: u32::from(supply >> 3) * 4,
        // Steps of 10 uV.
        shunt_microvolts: i32::from(shunt) * 10,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{Bus, MockI2c};

    #[test]
    fn a_reading_converts_and_only_pointers_are_written() {
        let mut bus = MockI2c::default();
        // Shunt 5.00 mV; bus 0.852 V (213 steps, flags clear).
        bus.answers.extend([vec![0x01, 0xF4], vec![0x06, 0xA8]]);
        let r = read(&mut bus, 0x40).unwrap();
        assert_eq!(
            r,
            Reading {
                bus_millivolts: 852,
                shunt_microvolts: 5000
            }
        );
        // 5 mV across 1 mOhm is 5 A: 4.26 W.
        assert_eq!(r.milliamps(1000), 5000);
        assert_eq!(r.milliwatts(1000), 4260);
        assert_eq!(
            bus.log,
            [
                Bus::Write(0x40, vec![1]),
                Bus::Read(0x40, 2),
                Bus::Write(0x40, vec![2]),
                Bus::Read(0x40, 2)
            ]
        );
    }

    #[test]
    fn current_may_be_negative() {
        let mut bus = MockI2c::default();
        bus.answers.extend([vec![0xFF, 0x9C], vec![0x00, 0x00]]);
        let r = read(&mut bus, 0x41).unwrap();
        assert_eq!(r.shunt_microvolts, -1000);
        assert_eq!(r.milliamps(10_000), -100);
    }
}
