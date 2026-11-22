//! Base overlay, RTOS: an HTTPS server (TLS 1.3) as one task among several; the 10 ms control loop keeps its deadline.
//!
//! Three tasks: `control` (priority 5) runs every 10 ms and records how far each release is from its time; `network` (priority 3)
//! owns the Ethernet port, the stack and the TLS connections, polls every millisecond and serves a status page, which shows the control loop's own
//! numbers; `report` (priority 2) prints them every 5 seconds. A TLS handshake costs about 100 ms of processor time on this board, all of it in the network task:
//! load the server from a PC (`while true; do curl -sk https://<address>/ >/dev/null; done`, or several at once) and watch the worst deviation of the control
//! loop stay put. The address is DHCP's, or 192.168.2.100 after eight seconds without a server. The certificate is the throwaway test identity of `fixtures/tls/`, and the random
//! bytes come from the clock only (no SD card here): **for a measurement, not for use**. Needs `hardroxide-net`, `hardroxide-crypto`, `hardroxide-tls` and the `net` feature of
//! `hardroxide-ps7`. Base overlay application 23.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{Periodic, delay_ms, spawn, start, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_crypto::Drbg;
use hardroxide_net::{Config, Stack, StaticConfig};
use hardroxide_tls::{Config as TlsConfig, HttpsServer};
use ps7::gem::{DMA_REGION_BYTES, Gem};
use ps7::println;

ps7::entry!(main);

const DMA_BASE: usize = 0x0F00_0000;
static RELEASES: AtomicU32 = AtomicU32::new(0);
static WORST_US: AtomicU32 = AtomicU32::new(0);
static REQUESTS: AtomicU32 = AtomicU32::new(0);
static CERT: &[u8] = include_bytes!("test-cert.der");
static KEY: &[u8; 32] = include_bytes!("test-key.bin");
/// The address the network task got, as a big-endian number (0 until it has one).
static ADDRESS: AtomicU32 = AtomicU32::new(0);

fn t0_ns() -> u64 {
    ps7::time::now_ns()
}

fn control() {
    let mut every = Periodic::every_ms(10);
    let mut last = Instant::now();
    loop {
        every.wait_next();
        let now = Instant::now();
        let period_us = now.duration_since(last).as_micros() as u32;
        last = now;
        if RELEASES.fetch_add(1, Relaxed) > 0 {
            WORST_US.fetch_max(period_us.abs_diff(10_000), Relaxed);
        }
    }
}

fn network() {
    // SAFETY: the SLCR is written once (the clock for the link speed), GEM 0 is used by nothing else, the DMA window is DDR that
    // nothing else uses, and the MMU is on.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    unsafe { ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap();
    match gem.link_up(&slcr, 15_000) {
        Ok(l) => println!("link {} Mbit/s {}", l.speed_mbps, if l.full_duplex { "full duplex" } else { "half duplex" }),
        Err(e) => {
            println!("no link: {e}");
            return;
        }
    }
    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [192, 168, 2, 100], netmask: [255, 255, 255, 0], gateway: Some([192, 168, 2, 1]) }), fallback_after_ms: 8000 };
    let t0 = Instant::now();
    let ms = || t0.elapsed().as_millis() as u64;
    let mut net: Stack<_, 4, 1, 2048> = Stack::new(gem, config, ms());
    let tls_config = TlsConfig::p256(CERT, *KEY);
    let mut drbg = Drbg::new(b"hardroxide rt_https");
    drbg.add_entropy(&t0_ns().to_le_bytes());
    let mut random = || {
        drbg.add_entropy(&t0_ns().to_le_bytes());
        drbg.bytes64()
    };
    let mut server: HttpsServer<4, 1024, 512> = HttpsServer::new(443, 0, &tls_config, &mut random);
    let mut announced = false;
    loop {
        net.poll(ms());
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!("NET RT READY: https://{}.{}.{}.{}/", ip[0], ip[1], ip[2], ip[3]);
            ADDRESS.store(u32::from_be_bytes(ip), Relaxed);
        }
        let stats = net.stats();
        server.poll(&mut net, &mut random, |req, res| {
            REQUESTS.fetch_add(1, Relaxed);
            match (req.method, req.path) {
                ("GET", "/") | ("GET", "/api/status") => res.text(
                    200,
                    "application/json",
                    format_args!(
                        "{{\"control_releases\":{},\"control_worst_deviation_us\":{},\"requests\":{},\"rx_frames\":{},\"tx_frames\":{}}}\n",
                        RELEASES.load(Relaxed),
                        WORST_US.load(Relaxed),
                        REQUESTS.load(Relaxed),
                        stats.rx_frames,
                        stats.tx_frames
                    ),
                ),
                _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
            }
        });
        delay_ms(1);
    }
}

fn report() {
    loop {
        delay_ms(5000);
        let ip = ADDRESS.load(Relaxed).to_be_bytes();
        println!(
            "control: {} releases, worst deviation from 10 ms {} us; web: https://{}.{}.{}.{}/ {} requests",
            RELEASES.load(Relaxed),
            WORST_US.load(Relaxed),
            ip[0],
            ip[1],
            ip[2],
            ip[3],
            REQUESTS.load(Relaxed)
        );
    }
}

fn main() -> ! {
    if let Err(e) = Design::init() {
        println!("Design::init failed: {e}");
        halt()
    }
    println!("\n== rt_https: an HTTPS server beside a 10 ms control loop ==");
    spawn("control", 5, task_stack!(8192).unwrap(), control).unwrap();
    spawn("network", 3, task_stack!(262144).unwrap(), network).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
