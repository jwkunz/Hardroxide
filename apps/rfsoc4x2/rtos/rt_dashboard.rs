//! RFSoC 4x2 base overlay on the real-time kernel: a web page for the board, beside a 10 ms control loop that keeps its
//! deadline.
//!
//! Three tasks; a higher number is more urgent:
//!
//! | task | priority | what it does |
//! |---|---|---|
//! | `control` | 5 | every 10 ms: copies the switches to the white LEDs (or shows what the page chose), sets the colour LEDs, reads the buttons and temperature, and records how far this release was from its time |
//! | `network` | 3 | owns the Ethernet port and the network stack, polls every millisecond, serves the page |
//! | `report` | 2 | every five seconds, prints the control loop's numbers and the address |
//!
//! Only `control` touches the FPGA. The tasks meet in a few atomic numbers, so the page can never hold the loop up.
//!
//! The address comes from DHCP, or after eight seconds without a server is 192.168.2.100. **There is no password**:
//! whoever can reach the address can set the LEDs. Needs `hardroxide-net` and the `net` feature of `hardroxide-psu`.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{Periodic, Shared, WAIT_FOREVER, delay_ms, spawn, start, task_stack};
use design_1::{Design, PS_CLK_HZ, halt, periph_hz, psu};
use hardroxide_dma::{DmaBuffer, static_dma_buffer};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};
use hardroxide_net::http::Server;
use hardroxide_net::{Config, Stack, StaticConfig};
use psu::gem::{DMA_REGION_BYTES, Gem};
use psu::println;

psu::entry!(main);

/// The system monitor's latest temperature reading.
const TEMPERATURE: usize = 0x400;
/// A locally administered address. Give two boards on one network different ones.
const MAC: [u8; 6] = [0x02, 0x48, 0x52, 0x58, 0x00, 0x01];

static DESIGN: Shared<Design<PhysMem>> = Shared::new();

// What `control` read last, for the page.
static BUTTONS: AtomicU32 = AtomicU32::new(0);
static SWITCHES: AtomicU32 = AtomicU32::new(0);
static LEDS: AtomicU32 = AtomicU32::new(0);
static MILLICELSIUS: AtomicU32 = AtomicU32::new(0);
// What the page chose, for `control`. The white LEDs follow the switches until a number is chosen.
const FOLLOW_SWITCHES: u32 = u32::MAX;
static CHOSEN_LEDS: AtomicU32 = AtomicU32::new(FOLLOW_SWITCHES);
static CHOSEN_COLOUR: AtomicU32 = AtomicU32::new(0);
// The numbers that show whether the loop kept its time.
static RELEASES: AtomicU32 = AtomicU32::new(0);
static WORST_US: AtomicU32 = AtomicU32::new(0);
static REQUESTS: AtomicU32 = AtomicU32::new(0);
/// The address the network task got, as a big-endian number (0 until it has one).
static ADDRESS: AtomicU32 = AtomicU32::new(0);

static PAGE: &[u8] = br#"<!doctype html><meta charset="utf-8"><title>RFSoC 4x2</title>
<style>body{font:16px sans-serif;max-width:34em;margin:2em auto;padding:0 1em}button{font:inherit;padding:.4em .8em;margin:.2em}
button.on{background:#2a7;color:#fff}</style>
<h1>RFSoC 4x2, real-time kernel</h1><p id="inputs">...</p><p>White LEDs: <span id="leds"></span></p>
<p>Colour LEDs: <span id="colour"></span></p><p id="loop"></p>
<script>
const names=["off","red","green","yellow","blue","magenta","cyan","white"];
const bits=(v,n)=>v.toString(2).padStart(n,"0");
async function set(what,value){await fetch(`/${what}?value=${value}`,{method:"POST"});show()}
function button(label,on,what,value){const b=document.createElement("button");b.textContent=label;if(on)b.className="on";b.onclick=()=>set(what,value);return b}
async function show(){
 const s=await (await fetch("/state")).json();
 inputs.textContent=`FPGA ${s.celsius.toFixed(1)} C. Buttons ${bits(s.buttons,4)}, switches ${bits(s.switches,4)}.`;
 const follow=button("follow switches",s.follow,"leds",16);
 leds.replaceChildren(...[0,1,2,3].map(i=>button(i,s.leds>>i&1,"leds",s.leds^1<<i)),follow);
 colour.replaceChildren(...names.map((n,i)=>button(n,s.colour==i,"colour",i)));
 loop.textContent=`Control loop: ${s.releases} releases, worst deviation from 10 ms ${s.worst_us} us. ${s.requests} requests served.`;
}
show();setInterval(show,1000);
</script>"#;

/// The colour LEDs' six bits for both LEDs showing the colour `rgb` (bit 0 red, 1 green, 2 blue): the overlay wires
/// each LED as blue, green, red.
fn colour_bits(rgb: u32) -> u32 {
    let bgr = (rgb & 1) << 2 | (rgb & 2) | (rgb & 4) >> 2;
    bgr | bgr << 3
}

fn control() {
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.leds_gpio.set_input_mask(Channel::One, 0).unwrap();
        d.rgbleds_gpio.set_input_mask(Channel::One, 0).unwrap();
    }
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
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        let switches = d.sws_gpio.read(Channel::One).unwrap() & 0xF;
        let chosen = CHOSEN_LEDS.load(Relaxed);
        let leds = if chosen == FOLLOW_SWITCHES {
            switches
        } else {
            chosen
        };
        d.leds_gpio.write(Channel::One, leds).unwrap();
        d.rgbleds_gpio
            .write(Channel::One, colour_bits(CHOSEN_COLOUR.load(Relaxed)))
            .unwrap();
        SWITCHES.store(switches, Relaxed);
        LEDS.store(leds, Relaxed);
        BUTTONS.store(d.btns_gpio.read(Channel::One).unwrap() & 0xF, Relaxed);
        let reading = d.system_management_wiz_0.read32(TEMPERATURE).unwrap() as u16;
        MILLICELSIUS.store(psu::sysmon::millicelsius(reading).max(0) as u32, Relaxed);
    }
}

/// The page, a piece at a time as the connection has room.
fn page(_: u32, offset: usize, out: &mut [u8]) -> usize {
    let rest = &PAGE[offset.min(PAGE.len())..];
    let n = rest.len().min(out.len());
    out[..n].copy_from_slice(&rest[..n]);
    n
}

fn network() {
    // The MAC's rings and buffers: memory the processor does not cache.
    let buffers = static_dma_buffer!(DMA_REGION_BYTES).unwrap();
    // SAFETY: GEM 1 (the board's Ethernet port) is used by this task alone, and the clock registers are written once,
    // for the link's speed.
    let be = unsafe { PhysMem::new() };
    let regs = be
        .map(psu::map::GEM1_BASE as u64, psu::map::GEM_SIZE)
        .unwrap();
    // SAFETY: the region is not cached, used for nothing else, aligned, below 4 GiB and kept for the program's life.
    let mut gem = unsafe { Gem::new(regs, buffers.phys_addr() as usize, MAC) };
    gem.init().unwrap();
    // The board's PHY needs its clock delays (receive 2.25 ns, transmit 2.75 ns).
    gem.dp83867_set_delays(0x8, 0xA).unwrap();
    let crl = be
        .map(psu::map::CRL_APB_BASE as u64, psu::map::CRL_APB_SIZE)
        .unwrap();
    let crf = be
        .map(psu::map::CRF_APB_BASE as u64, psu::map::CRF_APB_SIZE)
        .unwrap();
    // Waits up to eight seconds for the cable; `control` runs meanwhile.
    match psu::gem::link_up(&mut gem, &crl, &crf, PS_CLK_HZ, 1, 8000) {
        Ok(l) => println!(
            "link {} Mbit/s {}",
            l.speed_mbps,
            if l.full_duplex {
                "full duplex"
            } else {
                "half duplex"
            }
        ),
        Err(e) => {
            println!("no link: {e}");
            return;
        }
    }
    let config = Config::Dhcp {
        fallback: Some(StaticConfig {
            ip: [192, 168, 2, 100],
            netmask: [255, 255, 255, 0],
            gateway: Some([192, 168, 2, 1]),
        }),
        fallback_after_ms: 8000,
    };
    let t0 = Instant::now();
    let ms = || t0.elapsed().as_millis() as u64;
    let mut net: Stack<_, 4, 1, 2048> = Stack::new(gem, config, ms());
    let mut server: Server<4, 1024, 512> = Server::new(80, 0);
    let mut announced = false;
    loop {
        net.poll(ms());
        if net.is_configured() && !announced {
            announced = true;
            let ip = net.ip();
            println!(
                "RT DASHBOARD READY: http://{}.{}.{}.{}/",
                ip[0], ip[1], ip[2], ip[3]
            );
            ADDRESS.store(u32::from_be_bytes(ip), Relaxed);
        }
        server.poll(&mut net, |req, res| {
            REQUESTS.fetch_add(1, Relaxed);
            let value = req.param("value").and_then(|v| v.parse::<u32>().ok());
            match (req.method, req.path, value) {
                ("GET", "/", _) => {
                    res.generated(200, "text/html; charset=utf-8", PAGE.len(), page, 0)
                }
                ("GET", "/state", _) => {
                    let chosen = CHOSEN_LEDS.load(Relaxed);
                    let t = MILLICELSIUS.load(Relaxed);
                    res.text(
                        200,
                        "application/json",
                        format_args!(
                            "{{\"celsius\":{}.{},\"buttons\":{},\"switches\":{},\"leds\":{},\"follow\":{},\"colour\":{},\"releases\":{},\"worst_us\":{},\"requests\":{}}}\n",
                            t / 1000,
                            t % 1000 / 100,
                            BUTTONS.load(Relaxed),
                            SWITCHES.load(Relaxed),
                            LEDS.load(Relaxed),
                            chosen == FOLLOW_SWITCHES,
                            CHOSEN_COLOUR.load(Relaxed),
                            RELEASES.load(Relaxed),
                            WORST_US.load(Relaxed),
                            REQUESTS.load(Relaxed)
                        ),
                    )
                }
                // 16 gives the white LEDs back to the switches.
                ("POST", "/leds", Some(v @ 0..=16)) => {
                    CHOSEN_LEDS.store(if v == 16 { FOLLOW_SWITCHES } else { v }, Relaxed);
                    res.empty(204)
                }
                ("POST", "/colour", Some(v @ 0..=7)) => {
                    CHOSEN_COLOUR.store(v, Relaxed);
                    res.empty(204)
                }
                _ => res.text(404, "text/plain", format_args!("not found\n")),
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
            "control: {} releases, worst deviation from 10 ms {} us; web: http://{}.{}.{}.{}/ {} requests",
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
    // Checks that the FPGA is configured, sets up the processor and maps every block of the design.
    let design = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== RFSoC 4x2 base overlay: a web page beside a 10 ms control loop ==");
    DESIGN.set(design);
    spawn("control", 5, task_stack!(8192).unwrap(), control).unwrap();
    spawn("network", 3, task_stack!(65536).unwrap(), network).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(periph_hz().unwrap(), 1000)
}
