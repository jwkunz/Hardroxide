//! Base overlay, bare metal: bring up the Ethernet port, get an address and answer `ping`.
//!
//! The MAC (GEM 0) is initialised, the PHY autonegotiates, and the network stack asks for an address with DHCP; with no DHCP server
//! it falls back to 192.168.2.100 after eight seconds (a board plugged straight into a PC whose port is 192.168.2.1/24). The console
//! shows the link and the address; from the PC, `ping` the address. The frame counters print every five seconds. Plug the Ethernet
//! cable in first. Needs `hardroxide-net` and the `net` feature of `hardroxide-ps7`. Base overlay application 18.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_net::{Config, Stack, StaticConfig};
use ps7::gem::{DMA_REGION_BYTES, Gem};
use ps7::println;

ps7::entry!(main);

/// A MiB of DDR for the MAC's rings and buffers, made non-cacheable. (The video examples use 0x1000_0000 and up.)
const DMA_BASE: usize = 0x0F00_0000;

fn ms() -> u64 {
    ps7::time::now_ns() / 1_000_000
}

fn main() -> ! {
    let _d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== net_ping ==");
    // SAFETY: the SLCR is read and written only here (the clock for the link speed); GEM 0 is used by nothing else; the DMA
    // window is DDR that nothing else uses and the MMU is on.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    unsafe { ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap();
    println!("waiting for the link (plug in the cable)...");
    let link = match gem.link_up(&slcr, 15_000) {
        Ok(l) => l,
        Err(e) => {
            println!("NET PING FAILED: {e}");
            halt()
        }
    };
    println!("PHY {}: {} Mbit/s {}", gem.phy_address().unwrap(), link.speed_mbps, if link.full_duplex { "full duplex" } else { "half duplex" });

    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [192, 168, 2, 100], netmask: [255, 255, 255, 0], gateway: Some([192, 168, 2, 1]) }), fallback_after_ms: 8000 };
    let mut net: Stack<_, 1, 1, 1024> = Stack::new(gem, config, ms());
    let mut announced = false;
    let mut last_report = Instant::now();
    loop {
        net.poll(ms());
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!("NET PING READY: address {}.{}.{}.{} ({}) - ping it", ip[0], ip[1], ip[2], ip[3], if net.dhcp_bound() { "from DHCP" } else { "static fallback" });
        }
        if last_report.elapsed() > Duration::from_secs(5) {
            last_report = Instant::now();
            let s = net.stats();
            println!("frames in {} out {}, echo replies {}, bad {}, dropped {}", s.rx_frames, s.tx_frames, s.icmp_echo_replied, s.bad_packets, s.tx_dropped);
        }
    }
}
