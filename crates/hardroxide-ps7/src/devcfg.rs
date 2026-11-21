//! The device-configuration interface (`devcfg`): whether the PL has been configured.
//!
//! Touching a PL address before the PL is programmed never completes and hangs the
//! CPU, so programs check [`pl_configured`] first. (Programming the PL itself is done by
//! the loader or the FSBL; see ADR 0003.)

use hardroxide_mmio::{Mmio, MmioError};

/// Base of the device-configuration registers.
pub const DEVCFG_BASE: usize = 0xF800_7000;
pub const DEVCFG_SIZE: usize = 0x100;

const INT_STS: usize = 0x0C;
/// `PCFG_DONE`: the PL finished configuring (sticky until cleared by writing 1).
const INT_PCFG_DONE: u32 = 1 << 2;

/// Whether the programmable logic has been configured since the last PS reset.
pub fn pl_configured(devcfg: &impl Mmio) -> Result<bool, MmioError> {
    Ok(devcfg.read32(INT_STS)? & INT_PCFG_DONE != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn reports_the_done_flag() {
        let be = MockBackend::new();
        let r = be.map(DEVCFG_BASE as u64, DEVCFG_SIZE).unwrap();
        assert!(!pl_configured(&r).unwrap());
        be.poke(DEVCFG_BASE as u64 + INT_STS as u64, 0x4);
        assert!(pl_configured(&r).unwrap());
        be.poke(DEVCFG_BASE as u64 + INT_STS as u64, 0xFFFF_FFFB);
        assert!(!pl_configured(&r).unwrap());
    }
}
