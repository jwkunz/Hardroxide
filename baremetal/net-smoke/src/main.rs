//! End-to-end test of the Ethernet MAC driver and the network stack: bring the MAC up, get an address from DHCP, then serve
//! HTTP on port 80 and echo UDP on port 7 until stopped. Under QEMU (user-mode networking, the virtual router at 10.0.2.2
//! answers DHCP and forwards host ports) `scripts/test-qemu.sh` talks to it with `curl` and a UDP client.

#![no_std]
#![no_main]

use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};
use hardroxide_crypto::Drbg;
use hardroxide_net::http::Server;
use hardroxide_tls::{Config as TlsConfig, HttpsServer};
use hardroxide_net::{Config, Stack, StaticConfig};
use hardroxide_ps7::gem::{DMA_REGION_BYTES, Gem};
use hardroxide_ps7::{clocks, console, map, println, time};

hardroxide_ps7::entry!(main);

const PS_CLK_HZ: u64 = if cfg!(feature = "qemu") { 33_333_333 } else { 50_000_000 };
/// The DMA region for the MAC: a MiB of DDR made non-cacheable.
const DMA_BASE: usize = 0x0F00_0000;

fn platform_init() {
    // SAFETY: this program owns the SLCR for the duration of this setup.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let w = |o: usize, v: u32| slcr.write32(o, v).unwrap();
    let r = |o: usize| slcr.read32(o).unwrap();
    w(0x008, 0xDF0D);
    if cfg!(feature = "qemu") {
        let m = |a: usize, mask: u32, v: u32| w(a, (r(a) & !mask) | (v & mask));
        m(0x108, 0x0007_F000, 0x0002_0000);
        m(0x108, 0x10, 0x10);
        m(0x108, 0x1, 0x1);
        m(0x108, 0x1, 0x0);
        m(0x108, 0x10, 0x0);
    }
    w(0x12C, r(0x12C) | (1 << 20));
    let io_fdiv = (r(0x108) >> 12) & 0x7F;
    w(0x154, ((io_fdiv / 2).max(1) << 8) | 0x3);
    w(0x228, 0);
    w(0x738, 0x16E1);
    w(0x73C, 0x16E0);
    w(0x004, 0x767B);
}

fn ms() -> u64 {
    time::now_ns() / 1_000_000
}

/// A deterministic body for the large download.
fn pattern(arg: u32, offset: usize, out: &mut [u8]) -> usize {
    for (i, b) in out.iter_mut().enumerate() {
        *b = ((offset + i) as u32).wrapping_mul(2_654_435_761).wrapping_add(arg).rotate_left(7) as u8;
    }
    out.len()
}

type Net = Stack<Gem<hardroxide_mmio::PhysRegion>, 8, 1, 4096>;

/// The throwaway test identity (see fixtures/tls/README.md): never use it for anything real.
static TEST_CERT: &[u8] = include_bytes!("../../../fixtures/tls/test-cert.der");
static TEST_KEY: &[u8; 32] = include_bytes!("../../../fixtures/tls/test-key.bin");

fn main() -> ! {
    platform_init();
    // SAFETY: early in start-up, running from DDR at its physical address.
    unsafe { hardroxide_ps7::mem::enable(512 << 20) };
    // SAFETY: the SLCR is only read here; the console UART is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let uart_ref = clocks::uart_ref_hz(&slcr, PS_CLK_HZ).unwrap();
    // SAFETY: as above.
    unsafe { console::init(map::UART0_BASE, uart_ref as u32, 115_200) };
    let periph = clocks::periph_hz(&slcr, PS_CLK_HZ).unwrap();
    // SAFETY: nothing else touches the global timer.
    unsafe { time::init(periph) };
    println!("\n== Ethernet and network stack smoke test ==");

    // SAFETY: the region is DDR that nothing else uses; the MMU is on.
    unsafe { hardroxide_ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let regs = be.map(map::GEM0_BASE as u64, map::GEM_SIZE).unwrap();
    // SAFETY: the region above is not cached, not used for anything else and 64-byte aligned.
    let mut gem = unsafe { Gem::new(regs, DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap_or_else(|e| fail(&e));
    let link = gem.link_up(&slcr, 5000).unwrap_or_else(|e| fail(&e));
    println!("PHY at address {}, link {} Mbit/s {}", gem.phy_address().unwrap(), link.speed_mbps, if link.full_duplex { "full duplex" } else { "half duplex" });

    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [10, 0, 2, 50], netmask: [255, 255, 255, 0], gateway: Some([10, 0, 2, 2]) }), fallback_after_ms: 8000 };
    let mut net: Net = Stack::new(gem, config, ms());
    let mut server: Server<3, 2048, 1024> = Server::new(80, 0);
    net.tcp(3).listen(9); // a plain TCP echo, for a quick check
    // HTTPS on port 443 over sockets 4 to 7. The random bytes come from a generator fed with the clock (this smoke test has no better source).
    let mut drbg = Drbg::new(b"net-smoke: not a real seed");
    drbg.add_entropy(&ms().to_le_bytes());
    let tls_config = TlsConfig::p256(TEST_CERT, *TEST_KEY);
    
    let mut random = || {
        drbg.add_entropy(&time::now_ns().to_le_bytes());
        drbg.bytes64()
    };
    let mut https: HttpsServer<4, 2048, 1024> = HttpsServer::new(443, 4, &tls_config, &mut random);
    net.udp(0).bind(7);
    let mut announced = false;
    let mut hits = 0u32;
    loop {
        let now = ms();
        net.poll(now);
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!("NET READY ip={}.{}.{}.{} dhcp={} gateway={:?}", ip[0], ip[1], ip[2], ip[3], net.dhcp_bound(), net.gateway());
        }
        let stats = net.stats();
        server.poll(&mut net, |req, res| {
            hits += 1;
            match (req.method, req.path) {
                ("GET", "/") => res.text(200, "text/plain", format_args!("hello from the board\nrequests {hits}\nframes in {} out {}\n", stats.rx_frames, stats.tx_frames)),
                ("GET", "/big") => res.generated(200, "application/octet-stream", 300_000, pattern, 5),
                ("POST", "/echo") => {
                    let n = req.body.len().min(res.scratch().len());
                    res.scratch()[..n].copy_from_slice(&req.body[..n]);
                    res.scratch_body(200, "application/octet-stream", n);
                }
                _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
            }
        });
        https.poll(&mut net, &mut random, |req, res| match (req.method, req.path) {
            ("GET", "/") => res.text(200, "text/plain", format_args!("hello over TLS 1.3 from the board\n")),
            ("GET", "/big") => res.generated(200, "application/octet-stream", 100_000, pattern, 5),
            ("POST", "/echo") => {
                let n = req.body.len().min(res.scratch().len());
                res.scratch()[..n].copy_from_slice(&req.body[..n]);
                res.scratch_body(200, "application/octet-stream", n);
            }
            _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
        });
        // UDP echo.
        let mut buf = [0u8; 512];
        if let Some((from, port, n)) = net.udp(0).recv_from(&mut buf) {
            net.udp_send(now, 0, from, port, &buf[..n]);
        }
        // TCP echo.
        let n = net.tcp(3).recv(&mut buf);
        if n > 0 {
            net.tcp(3).send(&buf[..n]);
        }
        if net.tcp(3).peer_closed() {
            net.tcp(3).close();
        }
        if net.tcp(3).is_closed() {
            net.tcp(3).listen(9);
        }
    }
}

fn fail(e: &dyn core::fmt::Display) -> ! {
    println!("NET SMOKE FAILED: {e}");
    loop {
        hardroxide_ps7::cp15::wfe()
    }
}
