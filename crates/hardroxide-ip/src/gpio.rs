//! AXI GPIO (`xilinx.com:ip:axi_gpio`), PG144 register map.

use hardroxide_mmio::{Mmio, MmioError};

const DATA: [usize; 2] = [0x0, 0x8];
const TRI: [usize; 2] = [0x4, 0xC];

/// One of the two GPIO channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    One,
    Two,
}

impl Channel {
    fn idx(self) -> usize {
        match self {
            Channel::One => 0,
            Channel::Two => 1,
        }
    }
}

pub struct AxiGpio<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiGpio<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    /// Set pin directions: a `1` bit makes the pin an input, `0` an output
    /// (the register is a tri-state control).
    pub fn set_input_mask(&self, ch: Channel, mask: u32) -> Result<(), MmioError> {
        self.regs.write32(TRI[ch.idx()], mask)
    }

    pub fn input_mask(&self, ch: Channel) -> Result<u32, MmioError> {
        self.regs.read32(TRI[ch.idx()])
    }

    /// Drive output pins (bits for input pins are ignored by the hardware).
    pub fn write(&self, ch: Channel, value: u32) -> Result<(), MmioError> {
        self.regs.write32(DATA[ch.idx()], value)
    }

    /// Read pin levels (outputs read back the driven value).
    pub fn read(&self, ch: Channel) -> Result<u32, MmioError> {
        self.regs.read32(DATA[ch.idx()])
    }

    pub fn into_inner(self) -> R {
        self.regs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn channels_use_documented_offsets() {
        let be = MockBackend::new();
        let g = AxiGpio::new(be.map(0x4120_0000, 0x1000).unwrap());
        g.set_input_mask(Channel::One, 0).unwrap();
        g.write(Channel::One, 0b1010).unwrap();
        g.set_input_mask(Channel::Two, 0x3f).unwrap();
        be.poke(0x4120_0008, 0b01_0001);
        assert_eq!(be.peek(0x4120_0000), 0b1010);
        assert_eq!(be.peek(0x4120_000C), 0x3f);
        assert_eq!(g.read(Channel::Two).unwrap(), 0b01_0001);
    }
}
