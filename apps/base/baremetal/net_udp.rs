//! Base overlay, bare metal: a UDP echo (port 7) and a UDP log sink (port 9999) on the Ethernet port.
//!
//! Whatever arrives on port 7 is sent back to its sender. Whatever arrives on port 9999 is printed on the console and counted, so the
//! board can be a log collector: `echo hello | nc -u -w1 <address> 9999` from a PC. The address is shown at start (DHCP, or the
//! static fallback 192.168.2.100). Needs `hardroxide-net` and the `net` feature of `hardroxide-ps7`. Base overlay application 19.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_net::{Config, Stack, StaticConfig};
use ps7::gem::{DMA_REGION_BYTES, Gem};
use ps7::println;

ps7::entry!(main);

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
    println!("\n== net_udp ==");
    // SAFETY: as in `net_ping`.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    unsafe { ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap();
    if let Err(e) = gem.link_up(&slcr, 15_000) {
        println!("NET UDP FAILED: {e}");
        halt()
    }
    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [192, 168, 2, 100], netmask: [255, 255, 255, 0], gateway: Some([192, 168, 2, 1]) }), fallback_after_ms: 8000 };
    let mut net: Stack<_, 1, 2, 1024> = Stack::new(gem, config, ms());
    net.udp(0).bind(7);
    net.udp(1).bind(9999);
    let (mut echoed, mut logged, mut announced) = (0u32, 0u32, false);
    loop {
        let now = ms();
        net.poll(now);
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!("NET UDP READY: {}.{}.{}.{}, echo on 7, log sink on 9999", ip[0], ip[1], ip[2], ip[3]);
        }
        let mut buf = [0u8; 512];
        if let Some((from, port, n)) = net.udp(0).recv_from(&mut buf) {
            if net.udp_send(now, 0, from, port, &buf[..n]) {
                echoed += 1;
                println!("echo #{echoed}: {n} bytes to {}.{}.{}.{}:{port}", from[0], from[1], from[2], from[3]);
            }
        }
        if let Some((from, port, n)) = net.udp(1).recv_from(&mut buf) {
            logged += 1;
            let text = core::str::from_utf8(&buf[..n]).unwrap_or("(not text)");
            println!("log #{logged} from {}.{}.{}.{}:{port}: {}", from[0], from[1], from[2], from[3], text.trim_end());
        }
    }
}
