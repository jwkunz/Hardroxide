//! AXI GPIO (`xilinx.com:ip:axi_gpio`), PG144 register map.

use hardroxide_mmio::{Mmio, MmioError};

const DATA: [usize; 2] = [0x0, 0x8];
const TRI: [usize; 2] = [0x4, 0xC];
/// Global interrupt enable, interrupt enable and interrupt status registers.
const GIER: usize = 0x11C;
const IP_ISR: usize = 0x120;
const IP_IER: usize = 0x128;
const GIE: u32 = 1 << 31;

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

    /// Interrupt when any pin of channel `ch` changes (the IP must have been built with its interrupt
    /// enabled). Sets the global enable and the channel's enable bit.
    pub fn enable_interrupt(&self, ch: Channel) -> Result<(), MmioError> {
        let bit = 1 << ch.idx();
        let ier = self.regs.read32(IP_IER)?;
        self.regs.write32(IP_IER, ier | bit)?;
        self.regs.write32(GIER, GIE)
    }

    /// Stop channel `ch` from interrupting (the global enable is left as it is).
    pub fn disable_interrupt(&self, ch: Channel) -> Result<(), MmioError> {
        let ier = self.regs.read32(IP_IER)?;
        self.regs.write32(IP_IER, ier & !(1 << ch.idx()))
    }

    /// Whether channel `ch` has a latched pin change.
    pub fn interrupt_pending(&self, ch: Channel) -> Result<bool, MmioError> {
        Ok(self.regs.read32(IP_ISR)? & (1 << ch.idx()) != 0)
    }

    /// Clear channel `ch`'s latched interrupt. The status register toggles on a write of 1, so
    /// the bit is written only while it is set.
    pub fn clear_interrupt(&self, ch: Channel) -> Result<(), MmioError> {
        let bit = 1 << ch.idx();
        if self.regs.read32(IP_ISR)? & bit != 0 {
            self.regs.write32(IP_ISR, bit)?;
        }
        Ok(())
    }

    pub fn into_inner(self) -> R {
        self.regs
    }
}

#[cfg(all(test, feature = "std"))]
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

    #[test]
    fn interrupt_registers_follow_pg144() {
        let be = MockBackend::new();
        let g = AxiGpio::new(be.map(0x4120_0000, 0x1000).unwrap());
        g.enable_interrupt(Channel::Two).unwrap();
        assert_eq!(be.peek(0x4120_0128), 0b10);
        assert_eq!(be.peek(0x4120_011C), 1 << 31);
        g.enable_interrupt(Channel::One).unwrap();
        assert_eq!(be.peek(0x4120_0128), 0b11);
        g.disable_interrupt(Channel::One).unwrap();
        assert_eq!(be.peek(0x4120_0128), 0b10);
        // Nothing latched: nothing is written (a write of 1 would toggle the bit).
        assert!(!g.interrupt_pending(Channel::Two).unwrap());
        be.poke(0x4120_0120, 0);
        g.clear_interrupt(Channel::Two).unwrap();
        assert_eq!(be.peek(0x4120_0120), 0);
        be.poke(0x4120_0120, 0b10);
        assert!(g.interrupt_pending(Channel::Two).unwrap());
        assert!(!g.interrupt_pending(Channel::One).unwrap());
    }
}
