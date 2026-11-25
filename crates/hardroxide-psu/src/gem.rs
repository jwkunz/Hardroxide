//! The Ethernet MACs (`GEM0`..`GEM3`): `hardroxide-ps7`'s driver, re-exported, with the link bring-up of this family,
//! whose MAC clocks are set in `CRL_APB`.
//!
//! The rings and buffers [`Gem::new`] is given must be non-cacheable memory below 4 GiB
//! ([`crate::mem::set_non_cacheable`] or the `.dma` section).

pub use hardroxide_ps7::gem::*;

use crate::clocks;
use hardroxide_mmio::Mmio;

/// The reference clock a link of `speed_mbps` needs, in hertz.
pub fn reference_hz(speed_mbps: u16) -> u64 {
    match speed_mbps {
        1000 => 125_000_000,
        100 => 25_000_000,
        _ => 2_500_000,
    }
}

/// Restart autonegotiation, wait up to `timeout_ms` for the link, set MAC `instance`'s transmit clock for the negotiated
/// speed and turn the receiver and transmitter on. `crl` and `crf` are the clock register windows and `ps_ref_hz` the
/// board's reference, as for [`clocks`]. A PHY that needs settings of its own (RGMII delays) gets them before this.
pub fn link_up<R: Mmio>(
    gem: &mut Gem<R>,
    crl: &impl Mmio,
    crf: &impl Mmio,
    ps_ref_hz: u64,
    instance: usize,
    timeout_ms: u64,
) -> Result<Link, GemError> {
    let link = gem.negotiate(timeout_ms)?;
    clocks::set_gem_ref_hz(crl, crf, ps_ref_hz, instance, reference_hz(link.speed_mbps))?;
    gem.start(link)?;
    Ok(link)
}
