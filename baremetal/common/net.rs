//! The board's Ethernet port, brought up for the programs that run on both processor families: the Zynq-7000's `GEM0`
//! (PYNQ-Z2); on UltraScale+ the RFSoC 4x2's `GEM1` with its TI DP83867 PHY, or `GEM0` under QEMU.

use super::platform::ps::{self, println};
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use ps::gem::{DMA_REGION_BYTES, Gem, GemError, Link};

/// The MAC's rings and buffers: DDR the program does not otherwise use, made non-cacheable.
const DMA_BASE: usize = 0x0F00_0000;

/// Initialise the MAC, negotiate the link and start. Prints what it found.
pub fn bring_up(mac: [u8; 6]) -> Result<Gem<PhysRegion>, GemError> {
    // SAFETY: the region is DDR that nothing else uses; the MMU is on.
    unsafe { ps::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    // SAFETY: the MAC's registers and the clock registers are used by nothing else.
    let be = unsafe { PhysMem::new() };
    let regs = be.map(family::MAC_BASE as u64, ps::map::GEM_SIZE)?;
    // SAFETY: the region above is not cached, not used for anything else, 64-byte aligned and below 4 GiB.
    let mut gem = unsafe { Gem::new(regs, DMA_BASE, mac) };
    gem.init()?;
    let link: Link = family::link_up(&be, &mut gem)?;
    println!(
        "PHY at address {}, link {} Mbit/s {}",
        gem.phy_address().unwrap_or(32),
        link.speed_mbps,
        if link.full_duplex {
            "full duplex"
        } else {
            "half duplex"
        }
    );
    Ok(gem)
}

#[cfg(target_arch = "arm")]
mod family {
    use super::*;

    pub const MAC_BASE: usize = ps::map::GEM0_BASE;

    pub fn link_up(be: &PhysMem, gem: &mut Gem<PhysRegion>) -> Result<Link, GemError> {
        let slcr = be.map(ps::map::SLCR_BASE as u64, ps::map::SLCR_SIZE)?;
        gem.link_up(&slcr, 8000)
    }
}

#[cfg(target_arch = "aarch64")]
mod family {
    use super::*;

    pub const MAC_BASE: usize = if cfg!(feature = "qemu") {
        ps::map::GEM0_BASE
    } else {
        ps::map::GEM1_BASE
    };

    pub fn link_up(be: &PhysMem, gem: &mut Gem<PhysRegion>) -> Result<Link, GemError> {
        if cfg!(feature = "qemu") {
            // QEMU models neither the clock registers nor the PHY's settings.
            let link = gem.negotiate(8000)?;
            gem.start(link)?;
            return Ok(link);
        }
        // The RFSoC 4x2's RGMII clock delays (its device tree: receive 2.25 ns, transmit 2.75 ns).
        let found = gem.dp83867_set_delays(0x8, 0xA)?;
        println!(
            "PHY at address {}: {}",
            gem.phy_address().unwrap_or(32),
            if found {
                "DP83867, clock delays set"
            } else {
                "not a DP83867, left as it is"
            }
        );
        let crl = be.map(ps::map::CRL_APB_BASE as u64, ps::map::CRL_APB_SIZE)?;
        let crf = be.map(ps::map::CRF_APB_BASE as u64, ps::map::CRF_APB_SIZE)?;
        ps::gem::link_up(gem, &crl, &crf, 33_333_333, 1, 8000)
    }
}
