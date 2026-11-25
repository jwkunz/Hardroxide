//! End-to-end test of the Ethernet MAC driver and the network stack: bring the MAC up, get an address from DHCP, then serve
//! HTTP on port 80, HTTPS on 443 and echo TCP on port 9 and UDP on port 7 until stopped. Runs under QEMU (user-mode
//! networking, the virtual router at 10.0.2.2 answers DHCP and forwards host ports) and on a board; `scripts/net_check.py`
//! talks to it with `curl`, Python sockets and `openssl`.

#![no_std]
#![no_main]

use hardroxide_crypto::Drbg;
use hardroxide_net::http::Server;
use hardroxide_net::{Config, Stack, StaticConfig};
use hardroxide_tls::{Config as TlsConfig, HttpsServer};
use platform::ps::{self, cpu, gem::Gem, println, time};

#[path = "../../common/net.rs"]
mod net_port;
#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;

ps::entry!(main);

fn ms() -> u64 {
    time::now_ns() / 1_000_000
}

/// A deterministic body for the large download.
fn pattern(arg: u32, offset: usize, out: &mut [u8]) -> usize {
    for (i, b) in out.iter_mut().enumerate() {
        *b = ((offset + i) as u32)
            .wrapping_mul(2_654_435_761)
            .wrapping_add(arg)
            .rotate_left(7) as u8;
    }
    out.len()
}

type Net = Stack<Gem<hardroxide_mmio::PhysRegion>, 8, 1, 4096>;

/// The throwaway test identity (see fixtures/tls/README.md): never use it for anything real.
static TEST_CERT: &[u8] = include_bytes!("../../../fixtures/tls/test-cert.der");
static TEST_KEY: &[u8; 32] = include_bytes!("../../../fixtures/tls/test-key.bin");

fn main() -> ! {
    platform::init("Ethernet and network stack smoke test");
    let gem = net_port::bring_up([0x02, 0x48, 0x52, 0x58, 0x00, 0x01]).unwrap_or_else(|e| fail(&e));

    let config = Config::Dhcp {
        fallback: Some(StaticConfig {
            ip: [10, 0, 2, 50],
            netmask: [255, 255, 255, 0],
            gateway: Some([10, 0, 2, 2]),
        }),
        fallback_after_ms: 8000,
    };
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
            println!(
                "NET READY ip={}.{}.{}.{} dhcp={} gateway={:?}",
                ip[0],
                ip[1],
                ip[2],
                ip[3],
                net.dhcp_bound(),
                net.gateway()
            );
        }
        let stats = net.stats();
        server.poll(&mut net, |req, res| {
            hits += 1;
            match (req.method, req.path) {
                ("GET", "/") => res.text(
                    200,
                    "text/plain",
                    format_args!(
                        "hello from the board\nrequests {hits}\nframes in {} out {}\n",
                        stats.rx_frames, stats.tx_frames
                    ),
                ),
                ("GET", "/big") => {
                    res.generated(200, "application/octet-stream", 300_000, pattern, 5)
                }
                ("POST", "/echo") => {
                    let n = req.body.len().min(res.scratch().len());
                    res.scratch()[..n].copy_from_slice(&req.body[..n]);
                    res.scratch_body(200, "application/octet-stream", n);
                }
                _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
            }
        });
        https.poll(&mut net, &mut random, |req, res| {
            match (req.method, req.path) {
                ("GET", "/") => res.text(
                    200,
                    "text/plain",
                    format_args!("hello over TLS 1.3 from the board\n"),
                ),
                ("GET", "/big") => {
                    res.generated(200, "application/octet-stream", 100_000, pattern, 5)
                }
                ("POST", "/echo") => {
                    let n = req.body.len().min(res.scratch().len());
                    res.scratch()[..n].copy_from_slice(&req.body[..n]);
                    res.scratch_body(200, "application/octet-stream", n);
                }
                _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
            }
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
        cpu::wfe()
    }
}
