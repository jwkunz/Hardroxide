//! The programmable logic's state, from the configuration security unit (`map::CSU_BASE`).
//!
//! Loading a bitstream from the processor is not here: on bare metal the loader (`hardroxide run`, over JTAG) does it.

use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

const PCAP_STATUS: usize = 0x3010;
const PL_DONE: u32 = 1 << 3;

/// Whether the PL is configured. Touching a design's registers when it is not hangs the processor.
pub fn configured(csu: &impl Mmio) -> Result<bool> {
    Ok(csu.read32(PCAP_STATUS)? & PL_DONE != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn done_is_bit_3_of_the_status() {
        let be = MockBackend::new();
        let csu = be.map(0xFFCA_0000, 0x4000).unwrap();
        assert!(!configured(&csu).unwrap());
        be.poke(0xFFCA_3010, 0x7);
        assert!(!configured(&csu).unwrap());
        be.poke(0xFFCA_3010, 0xF);
        assert!(configured(&csu).unwrap());
    }
}
