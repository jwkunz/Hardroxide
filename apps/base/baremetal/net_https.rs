//! Base overlay, bare metal: an HTTPS server (TLS 1.3) on the board.
//!
//! Open `https://<address>/` (the browser warns about the self-signed certificate; `curl -k` does not care). The certificate is an ECDSA P-256 one, because browsers do not accept Ed25519 ones. The page and `/api/status` show the board's state, `/led?v=N`
//! sets the LEDs. The certificate and key are the **throwaway test identity** of `fixtures/tls/` (its private key is public: it protects nothing; make your own with
//! `hardroxide make-cert` and replace the two `include_bytes!` (the certificate in DER and the 32-byte private scalar)). The program times the public-key operations of a handshake on this board and prints them.
//!
//! Where the unpredictable bytes come from (the Zynq has no hardware random number generator): a generator is seeded from the clock, mixes in the timing of every new connection, and,
//! if a FAT32 card is in the slot, a 32-byte seed read from `/RNGSEED`. **The card is only read** unless it was named when building
//! (`HRX_WRITE_SERIAL=0x...`, the serial number `sd_info` prints): then the seed is replaced from the generator at every start, so entropy accumulates across runs. **That is weak**: without a seed
//! there is only the clock and the connection timing; see the reference page. Needs `hardroxide-net`, `hardroxide-crypto`, `hardroxide-tls` and `hardroxide-fat` (the card is optional at run time).
//! Base overlay application 22.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_crypto::{Drbg, ecdsa_p256_sign, p256_public, x25519_public};
use hardroxide_fat::{OpenMode, Volume};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_net::{Config, Stack, StaticConfig};
use hardroxide_tls::{Config as TlsConfig, HttpsServer};
use ps7::gem::{DMA_REGION_BYTES, Gem};
use ps7::println;
use ps7::sd::SdHost;

ps7::entry!(main);

const DMA_BASE: usize = 0x0F00_0000;
/// The throwaway test identity: replace with your own certificate and private key.
static CERT: &[u8] = include_bytes!("test-cert.der");
static KEY: &[u8; 32] = include_bytes!("test-key.bin");

fn ms() -> u64 {
    ps7::time::now_ns() / 1_000_000
}

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== net_https ==");
    // SAFETY: as in `net_ping`; SDIO 0 is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();

    // Entropy: the clock now, then a seed kept on the card.
    let mut drbg = Drbg::new(b"hardroxide net_https");
    drbg.add_entropy(&ps7::time::now_ns().to_le_bytes());
    let base = ps7::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32;
    let mut sd = SdHost::new(be.map(ps7::map::SDIO0_BASE as u64, ps7::map::SDIO_SIZE).unwrap(), base);
    let mut seeded_from_card = false;
    if let Ok(card) = sd.init()
        && let Ok(mut vol) = Volume::mount(sd)
    {
        let mut old = [0u8; 64];
        if let Ok(mut f) = vol.open("/RNGSEED", OpenMode::Read) {
            let n = f.read(&mut vol, &mut old).unwrap_or(0);
            drbg.add_entropy(&old[..n]);
            seeded_from_card = n >= 32;
        }
        // A fresh seed for next time (made after the old one was mixed in, so it depends on both it and this run's
        // timing), written only to the card named when building.
        if card.serial_matches(option_env!("HRX_WRITE_SERIAL")) {
            drbg.add_entropy(&ps7::time::now_ns().to_le_bytes());
            let mut fresh = [0u8; 32];
            drbg.fill(&mut fresh);
            if let Ok(mut f) = vol.open("/RNGSEED", OpenMode::Write) {
                let _ = f.write(&mut vol, &fresh);
                let _ = f.close(&mut vol);
                let _ = vol.sync();
            }
        } else {
            println!("the card's seed is read but not renewed: build with HRX_WRITE_SERIAL={:#010x} to let this program write to this card", card.serial);
        }
    }
    println!("random generator: {}", if seeded_from_card { "seeded from the clock and the seed file on the card" } else { "seeded from the clock only (no seed file yet: weak)" });

    // The cost of a handshake's public-key operations on this board.
    let t0 = ps7::time::now_ns();
    let public = p256_public(KEY).expect("a valid P-256 key");
    let t1 = ps7::time::now_ns();
    let eph = x25519_public(&drbg.bytes64()[..32].try_into().unwrap());
    let t2 = ps7::time::now_ns();
    let sig = ecdsa_p256_sign(KEY, &hardroxide_crypto::sha256(b"timing"));
    let t3 = ps7::time::now_ns();
    core::hint::black_box((public, eph, sig));
    println!("P-256 public key {} ms, X25519 {} ms, ECDSA signature {} ms (a handshake needs two X25519 and one signature)", (t1 - t0) / 1_000_000, (t2 - t1) / 1_000_000, (t3 - t2) / 1_000_000);

    unsafe { ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap();
    println!("waiting for the link (plug in the cable)...");
    if let Err(e) = gem.link_up(&slcr, 15_000) {
        println!("NET HTTPS FAILED: {e}");
        halt()
    }
    let (leds, sw, btns) = (&d.leds_gpio, &d.switches_gpio, &d.btns_gpio);
    leds.set_input_mask(Channel::One, 0).unwrap();

    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [192, 168, 2, 100], netmask: [255, 255, 255, 0], gateway: Some([192, 168, 2, 1]) }), fallback_after_ms: 8000 };
    let mut net: Stack<_, 4, 1, 2048> = Stack::new(gem, config, ms());
    let tls_config = TlsConfig::p256(CERT, *KEY);
    let mut random = || {
        drbg.add_entropy(&ps7::time::now_ns().to_le_bytes());
        drbg.bytes64()
    };
    let mut server: HttpsServer<4, 1024, 768> = HttpsServer::new(443, 0, &tls_config, &mut random);
    let (mut led_state, mut announced) = (0u32, false);
    loop {
        let now = ms();
        net.poll(now);
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!("NET HTTPS READY: https://{}.{}.{}.{}/ ({})", ip[0], ip[1], ip[2], ip[3], if net.dhcp_bound() { "from DHCP" } else { "static fallback" });
        }
        let stats = net.stats();
        let ip = net.ip();
        let served = server.served();
        server.poll(&mut net, &mut random, |req, res| match (req.method, req.path) {
            ("GET", "/") => res.text(
                200,
                "text/html; charset=utf-8",
                format_args!("<!doctype html><title>Hardroxide HTTPS</title><h1>Hardroxide on the PYNQ-Z2</h1><p>This page came over TLS 1.3 from bare-metal Rust with our own TCP/IP and TLS.</p><p><a href=/api/status>status</a> &middot; <a href='/led?v=5'>LEDs 0101</a> &middot; <a href='/led?v=0'>LEDs off</a></p>"),
            ),
            ("GET" | "POST", "/led") => match req.param("v").and_then(|v| v.parse::<u32>().ok()) {
                Some(v) if v < 16 => {
                    led_state = v;
                    leds.write(Channel::One, v).unwrap();
                    res.text(200, "text/plain", format_args!("LEDs {v:04b}\n"));
                }
                _ => res.text(400, "text/plain", format_args!("use /led?v=N with N from 0 to 15\n")),
            },
            ("GET", "/api/status") => res.text(
                200,
                "application/json",
                format_args!(
                    "{{\"ip\":\"{}.{}.{}.{}\",\"uptime_ms\":{now},\"leds\":{led_state},\"switches\":{},\"buttons\":{},\"https_requests\":{served},\"rx_frames\":{},\"tx_frames\":{}}}\n",
                    ip[0],
                    ip[1],
                    ip[2],
                    ip[3],
                    sw.read(Channel::One).unwrap_or(0) & 3,
                    btns.read(Channel::One).unwrap_or(0) & 15,
                    stats.rx_frames,
                    stats.tx_frames
                ),
            ),
            _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
        });
    }
}
