//! Base overlay, bare metal: a web server on the board.
//!
//! Open `http://<address>/` in a browser (the address is shown on the console: DHCP, or the static fallback 192.168.2.100 for a board
//! plugged straight into a PC at 192.168.2.1/24). The page shows the LEDs, switches and buttons live and has buttons that set the LEDs;
//! `/api/status` is the same as JSON, `/led?v=N` sets the four LEDs from a number 0 to 15 (`curl http://<address>/led?v=5`),
//! `/picture.bmp` is a small picture drawn with the graphics library on each request (uptime and the LEDs as circles). Five
//! connections are served at once. Needs `hardroxide-net`, `hardroxide-gfx` and the `net` feature of `hardroxide-ps7`.
//! Base overlay application 20.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_gfx::bmp::header_24bit;
use hardroxide_gfx::{Color, Surface};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_net::http::Server;
use hardroxide_net::{Config, Stack, StaticConfig};
use ps7::gem::{DMA_REGION_BYTES, Gem};
use ps7::println;

ps7::entry!(main);

const DMA_BASE: usize = 0x0F00_0000;
const PW: usize = 320;
const PH: usize = 120;
static mut PICTURE: [u32; PW * PH] = [0; PW * PH];

const PAGE: &[u8] = b"<!doctype html><html><head><meta charset=utf-8><meta name=viewport content='width=device-width'>\
<title>Hardroxide board</title><style>body{font-family:sans-serif;margin:2em;max-width:36em}\
button{font-size:1.2em;margin:.2em;padding:.4em .8em}.on{background:#e33;color:#fff}pre{background:#eee;padding:.6em}</style></head><body>\
<h1>Hardroxide on the PYNQ-Z2</h1><p>A web server written in Rust, running on bare metal: no operating system, our own TCP/IP stack.</p>\
<h3>LEDs</h3><p id=leds></p><h3>Status</h3><pre id=s>loading...</pre><img src=/picture.bmp width=640 alt=picture>\
<script>let v=0;function set(n){v^=1<<n;fetch('/led?v='+v).then(r=>r.text()).then(refresh)}\
function refresh(){fetch('/api/status').then(r=>r.json()).then(j=>{v=j.leds;\
document.getElementById('leds').innerHTML=[0,1,2,3].map(n=>\"<button class='\"+((j.leds>>n)&1?'on':'')+\"' onclick='set(\"+n+\")'>LED \"+n+'</button>').join('');\
document.getElementById('s').textContent=JSON.stringify(j,null,1)})}\
refresh();setInterval(refresh,1000)</script></body></html>";

fn ms() -> u64 {
    ps7::time::now_ns() / 1_000_000
}

/// The picture as a 24-bit BMP, produced in pieces from `PICTURE`.
fn bmp_bytes(_arg: u32, offset: usize, out: &mut [u8]) -> usize {
    let header = header_24bit(PW as u32, PH as u32);
    let row_bytes = (PW * 3).next_multiple_of(4);
    // SAFETY: PICTURE is written only by the handler before the response starts and read here while it is sent; the program has one thread.
    let pixels = unsafe { &*core::ptr::addr_of!(PICTURE) };
    for (i, o) in out.iter_mut().enumerate() {
        let at = offset + i;
        *o = if at < 54 {
            header[at]
        } else {
            let p = at - 54;
            let (row, col) = (p / row_bytes, p % row_bytes);
            let y = PH - 1 - row; // stored bottom to top
            match (col / 3 < PW).then(|| pixels[y * PW + col / 3]) {
                Some(px) => (px >> (8 * (col % 3))) as u8, // B, G, R
                None => 0,
            }
        };
    }
    out.len()
}

fn draw_picture(uptime_s: u64, leds: u32, requests: u32) {
    // SAFETY: see `bmp_bytes`.
    let pixels = unsafe { &mut *core::ptr::addr_of_mut!(PICTURE) };
    let mut s = Surface::new(pixels, PW, PH);
    s.clear(Color::rgb(16, 16, 32));
    for x in 0..PW as i32 {
        s.vline(x, 0, 8, Color::hue((x * 255 / PW as i32) as u8));
    }
    s.text(10, 16, "HARDROXIDE", Color::WHITE, None, 3);
    let mut line = [0u8; 48];
    let n = fmt(&mut line, format_args!("uptime {uptime_s} s, request {requests}"));
    s.text(10, 48, core::str::from_utf8(&line[..n]).unwrap_or("?"), Color::YELLOW, None, 1);
    for i in 0..4 {
        let (cx, cy) = (50 + i * 70, 90);
        if leds >> i & 1 != 0 {
            s.fill_circle(cx, cy, 18, Color::RED);
        }
        s.circle(cx, cy, 20, Color::WHITE);
    }
}

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== net_web ==");
    // SAFETY: as in `net_ping`.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    unsafe { ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap();
    println!("waiting for the link (plug in the cable)...");
    if let Err(e) = gem.link_up(&slcr, 15_000) {
        println!("NET WEB FAILED: {e}");
        halt()
    }
    let (leds, sw, btns) = (&d.leds_gpio, &d.switches_gpio, &d.btns_gpio);
    leds.set_input_mask(Channel::One, 0).unwrap();

    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [192, 168, 2, 100], netmask: [255, 255, 255, 0], gateway: Some([192, 168, 2, 1]) }), fallback_after_ms: 8000 };
    let mut net: Stack<_, 5, 1, 2048> = Stack::new(gem, config, ms());
    let mut server: Server<5, 1024, 768> = Server::new(80, 0);
    let (mut led_state, mut announced) = (0u32, false);
    loop {
        let now = ms();
        net.poll(now);
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!("NET WEB READY: http://{}.{}.{}.{}/ ({})", ip[0], ip[1], ip[2], ip[3], if net.dhcp_bound() { "from DHCP" } else { "static fallback" });
        }
        let stats = net.stats();
        let ip = net.ip();
        let served = server.served;
        server.poll(&mut net, |req, res| match (req.method, req.path) {
            ("GET", "/") => res.bytes(200, "text/html; charset=utf-8", PAGE),
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
                    "{{\"ip\":\"{}.{}.{}.{}\",\"uptime_ms\":{now},\"leds\":{led_state},\"switches\":{},\"buttons\":{},\"requests\":{served},\"rx_frames\":{},\"tx_frames\":{},\"tcp_segments_in\":{},\"tcp_resets_sent\":{}}}\n",
                    ip[0],
                    ip[1],
                    ip[2],
                    ip[3],
                    sw.read(Channel::One).unwrap_or(0) & 3,
                    btns.read(Channel::One).unwrap_or(0) & 15,
                    stats.rx_frames,
                    stats.tx_frames,
                    stats.tcp_segments_in,
                    stats.tcp_resets_sent
                ),
            ),
            ("GET", "/picture.bmp") => {
                draw_picture(now / 1000, led_state, served);
                res.generated(200, "image/bmp", 54 + (PW * 3).next_multiple_of(4) * PH, bmp_bytes, 0);
            }
            _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
        });
    }
}

/// Format into a byte buffer; returns the length.
fn fmt(buf: &mut [u8], args: core::fmt::Arguments) -> usize {
    use core::fmt::Write;
    struct W<'a>(&'a mut [u8], usize);
    impl Write for W<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let n = s.len().min(self.0.len() - self.1);
            self.0[self.1..self.1 + n].copy_from_slice(&s.as_bytes()[..n]);
            self.1 += n;
            Ok(())
        }
    }
    let mut w = W(buf, 0);
    let _ = w.write_fmt(args);
    w.1
}
