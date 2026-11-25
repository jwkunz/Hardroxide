//! The identity data of a Microchip AT24MAC402: a factory-programmed Ethernet address (EUI-48) and 128-bit serial
//! number, which boards use to identify themselves.
//!
//! Read only. The only byte written is the address to read from; the part's user memory (at another I2C address) is
//! not touched.

use core::fmt;
use embedded_hal::i2c::I2c;

/// The I2C address of the identity data when the part's address pins are low.
pub const IDENTITY_ADDRESS: u8 = 0x58;
const SERIAL_NUMBER: u8 = 0x80;
const EUI48: u8 = 0x9A;

/// An Ethernet address; displays as `aa:bb:cc:dd:ee:ff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Whether this is a usable address of one station: not all zeros, and not a group (multicast) address, which
    /// includes the all-ones of an unreadable bus.
    pub fn is_unicast(&self) -> bool {
        self.0 != [0; 6] && self.0[0] & 1 == 0
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{g:02x}")
    }
}

fn read_at<I: I2c>(bus: &mut I, address: u8, from: u8, back: &mut [u8]) -> Result<(), I::Error> {
    bus.write(address, &[from])?;
    bus.read(address, back)
}

/// The Ethernet address stored in the part at `address` ([`IDENTITY_ADDRESS`]).
pub fn mac_address<I: I2c>(bus: &mut I, address: u8) -> Result<MacAddress, I::Error> {
    let mut mac = [0; 6];
    read_at(bus, address, EUI48, &mut mac)?;
    Ok(MacAddress(mac))
}

/// The part's 128-bit serial number.
pub fn serial_number<I: I2c>(bus: &mut I, address: u8) -> Result<[u8; 16], I::Error> {
    let mut serial = [0; 16];
    read_at(bus, address, SERIAL_NUMBER, &mut serial)?;
    Ok(serial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{Bus, MockI2c};

    #[test]
    fn reading_the_identity_writes_only_the_address() {
        let mut bus = MockI2c::default();
        bus.answers
            .extend([vec![0x02, 0x11, 0x22, 0x33, 0x44, 0x55], vec![7; 16]]);
        let mac = mac_address(&mut bus, IDENTITY_ADDRESS).unwrap();
        assert_eq!(mac.to_string(), "02:11:22:33:44:55");
        assert!(mac.is_unicast());
        assert_eq!(serial_number(&mut bus, IDENTITY_ADDRESS).unwrap(), [7; 16]);
        assert_eq!(
            bus.log,
            [
                Bus::Write(0x58, vec![0x9A]),
                Bus::Read(0x58, 6),
                Bus::Write(0x58, vec![0x80]),
                Bus::Read(0x58, 16)
            ]
        );
    }

    #[test]
    fn blank_and_group_addresses_are_not_usable() {
        assert!(!MacAddress([0; 6]).is_unicast());
        assert!(!MacAddress([0xFF; 6]).is_unicast());
    }
}
