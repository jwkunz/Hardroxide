//! The SD host controller of the slot (controller 0), for the programs that run on both processor families.

use super::platform::ps;
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use ps::sd::SdHost;

/// The controller's input clock (QEMU: what its model assumes).
fn clock_hz() -> u32 {
    if cfg!(feature = "qemu") {
        return 50_000_000;
    }
    // SAFETY: the clock registers are only read.
    let be = unsafe { PhysMem::new() };
    #[cfg(target_arch = "aarch64")]
    {
        let crl = be
            .map(ps::map::CRL_APB_BASE as u64, ps::map::CRL_APB_SIZE)
            .unwrap();
        let crf = be
            .map(ps::map::CRF_APB_BASE as u64, ps::map::CRF_APB_SIZE)
            .unwrap();
        ps::clocks::sdio_ref_hz(&crl, &crf, 33_333_333, 0).unwrap() as u32
    }
    #[cfg(target_arch = "arm")]
    {
        let slcr = be
            .map(ps::map::SLCR_BASE as u64, ps::map::SLCR_SIZE)
            .unwrap();
        ps::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32
    }
}

/// The driver for the slot's controller, and its input clock.
pub fn host() -> (SdHost<PhysRegion>, u32) {
    // SAFETY: the controller's window is used by nothing else; programs call this once.
    let be = unsafe { PhysMem::new() };
    let base = clock_hz();
    let regs = be
        .map(ps::map::SDIO0_BASE as u64, ps::map::SDIO_SIZE)
        .unwrap();
    (SdHost::new(regs, base), base)
}
