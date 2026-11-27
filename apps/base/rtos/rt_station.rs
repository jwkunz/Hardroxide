//! Base overlay, RTOS: everything at once. An HTTPS dashboard controls HDMI graphics, audio from the SD card and the LEDs, while a 10 ms
//! control loop keeps its deadline; the page shows each task's share of the processor.
//!
//! Tasks: `control` (priority 6, every 10 ms, records its release jitter); the audio pump (a timer interrupt at twice the sample rate moving frames from a ring to the
//! codec, see `rt_audio`) fed by `feed` (4); `display` (3, 1280x720 at about 60 frames per second, a ball over bars, a gradient or a plain colour, and a status panel);
//! `network` (3, Ethernet, TCP/IP, TLS 1.3 and the web pages); `hog` (1, a processor burner the page can switch on); `report` (2, the console). Open `https://<address>/`
//! (the certificate is the throwaway test one of `fixtures/tls/`: **for a measurement, not for use**); the page shows the status and has the controls. `/api/status` is
//! JSON, `/led?v=N`, `/audio?cmd=play|stop|mute|unmute|next`, `/volume?v=0..62`, `/display?mode=0..2`, `/load?on=0|1`, `/screen.bmp` (the screen, a quarter of its size).
//! The SD card is optional: put `.wav` files (16-bit PCM) in its root and `/audio?cmd=next` steps through them; without a card the built-in chime plays.
//! **The card is only read**, unless it was named when building (`HRX_WRITE_SERIAL=0x...`): then the random seed in `/RNGSEED` is renewed at every start.
//! Copy `apps/base/data/chime.wav`, `test-cert.der` and `test-key.bin` to the crate's `examples/`. Needs `hardroxide-audio`, `-gfx`, `-video`, `-net`, `-crypto`, `-tls` and `-fat`,
//! and the `net` feature of `hardroxide-ps7`. Base overlay application 25.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{Periodic, Shared, WAIT_FOREVER, cpu_total_ticks, delay_ms, irq_ticks, spawn, start, task_info, task_slots, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_audio::SAMPLE_RATE_HZ;
use hardroxide_audio::adau1761::Adau1761;
use hardroxide_audio::pump::Pump;
use hardroxide_audio::resample::Resample;
use hardroxide_audio::wav::Wav;
use hardroxide_crypto::Drbg;
use hardroxide_fat::{OpenMode, Volume};
use hardroxide_gfx::bmp::header_24bit;
use hardroxide_gfx::{Color, Surface};
use hardroxide_ip::audio::AudioCodecCtrl;
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use hardroxide_net::{Config, Stack, StaticConfig};
use hardroxide_tls::{Config as TlsConfig, HttpsServer};
use hardroxide_video::{Display, HD_1280X720_60 as MODE, as_pixels};
use ps7::gem::{DMA_REGION_BYTES, Gem};
use ps7::gic::Gic;
use ps7::println;
use ps7::sd::SdHost;
use ps7::timer::GlobalTimer;

ps7::entry!(main);

// ---- Memory map and shared state ----
const DMA_BASE: usize = 0x0F00_0000;
const FB_BASE: usize = 0x1000_0000;
const FB_STRIDE: usize = 4 << 20;
const W: usize = 1280;
const H: usize = 720;
const FRAME_BYTES: usize = W * H * 4;

static CERT: &[u8] = include_bytes!("test-cert.der");
static KEY: &[u8; 32] = include_bytes!("test-key.bin");
static CHIME: &[u8] = include_bytes!("chime.wav");

static DESIGN: Shared<Design<PhysMem>> = Shared::new();
static PUMP: Pump<4096> = Pump::new();

// Control loop.
static RELEASES: AtomicU32 = AtomicU32::new(0);
static WORST_US: AtomicU32 = AtomicU32::new(0);
// Display.
static FRAMES: AtomicU32 = AtomicU32::new(0);
static DRAW_WORST_US: AtomicU32 = AtomicU32::new(0);
static MODE_SEL: AtomicU32 = AtomicU32::new(0);
static MODE_GEN: AtomicU32 = AtomicU32::new(1);
// Audio.
static AUDIO_PLAYING: AtomicU32 = AtomicU32::new(1);
static AUDIO_GEN: AtomicU32 = AtomicU32::new(1); // bumped when the sound changes: the feeder starts over
static WAV_LEN: AtomicU32 = AtomicU32::new(0); // 0: use the built-in chime
static VOLUME: AtomicU32 = AtomicU32::new(50);
// Web.
static REQUESTS: AtomicU32 = AtomicU32::new(0);
static ADDRESS: AtomicU32 = AtomicU32::new(0);
static LOAD_ON: AtomicU32 = AtomicU32::new(0);
static LEDS: AtomicU32 = AtomicU32::new(0);
static SD_MIB: AtomicU32 = AtomicU32::new(0);

const WAV_MAX: usize = 512 * 1024;
static mut WAV_BUF: [u8; WAV_MAX] = [0; WAV_MAX];

// ---- The audio interrupt ----
struct IsrState {
    ctrl: AudioCodecCtrl<PhysRegion>,
    timer: GlobalTimer<PhysRegion>,
}
static mut ISR: Option<IsrState> = None;

fn audio_isr() {
    // SAFETY: ISR is written once, before the interrupt is enabled, and only read afterwards.
    if let Some(st) = unsafe { (*core::ptr::addr_of!(ISR)).as_ref() } {
        let _ = st.timer.clear_event();
        PUMP.service(&st.ctrl);
    }
}

/// The sound: the card's file in `WAV_BUF` if one was loaded, else the built-in chime.
fn sound() -> &'static [u8] {
    let n = WAV_LEN.load(Relaxed) as usize;
    if n == 0 {
        CHIME
    } else {
        // SAFETY: WAV_BUF is rewritten only by the network task, which first sets WAV_LEN to 0 and bumps AUDIO_GEN so the feeder lets go of it.
        unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(WAV_BUF) as *const u8, n) }
    }
}

fn feed() {
    // SAFETY: the GIC window is shared with the kernel, which touches it only with interrupts masked; the global timer belongs to the handler from here on.
    let be = unsafe { PhysMem::new() };
    let gic = Gic::new(
        be.map(ps7::map::GIC_DIST_BASE as u64, ps7::map::GIC_DIST_SIZE).unwrap(),
        be.map(ps7::map::GIC_CPU_BASE as u64, ps7::map::GIC_CPU_SIZE).unwrap(),
    );
    let timer = GlobalTimer::new(be.map(ps7::map::GLOBAL_TIMER_BASE as u64, ps7::map::GLOBAL_TIMER_SIZE).unwrap());
    let ctrl = AudioCodecCtrl::new(be.map(design_1::pac::audio_codec_ctrl_0::BASE_ADDRESS, design_1::pac::audio_codec_ctrl_0::SIZE).unwrap());
    // SAFETY: nothing else touches ISR before the interrupt is enabled below.
    unsafe { *core::ptr::addr_of_mut!(ISR) = Some(IsrState { ctrl, timer: GlobalTimer::new(be.map(ps7::map::GLOBAL_TIMER_BASE as u64, ps7::map::GLOBAL_TIMER_SIZE).unwrap()) }) };
    ps7::irq::register(ps7::map::ppi::GLOBAL_TIMER, audio_isr);
    gic.set_priority(ps7::map::ppi::GLOBAL_TIMER, 0x40).unwrap();
    gic.enable(ps7::map::ppi::GLOBAL_TIMER).unwrap();
    let period = (design_1::periph_hz().unwrap() as f64 / (2.0 * SAMPLE_RATE_HZ)) as u32;
    timer.start_periodic_interrupt(period).unwrap();
    let mut gen_seen = 0;
    loop {
        if AUDIO_GEN.load(Relaxed) != gen_seen {
            gen_seen = AUDIO_GEN.load(Relaxed);
            PUMP.flush();
        }
        let snd = sound();
        let Ok(wav) = Wav::parse(snd) else {
            delay_ms(100);
            continue;
        };
        let mut frames = Resample::new(wav.frames(), wav.sample_rate, SAMPLE_RATE_HZ as u32);
        'play: loop {
            if AUDIO_GEN.load(Relaxed) != gen_seen {
                break 'play;
            }
            PUMP.set_muted(AUDIO_PLAYING.load(Relaxed) != 1);
            while PUMP.space() > 0 {
                match frames.next() {
                    Some(f) => {
                        PUMP.push(f);
                    }
                    None => frames = Resample::new(wav.frames(), wav.sample_rate, SAMPLE_RATE_HZ as u32), // loop
                }
            }
            delay_ms(5);
        }
    }
}

// ---- The control loop and the load ----
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

fn hog() {
    let mut x = 1u32;
    loop {
        if LOAD_ON.load(Relaxed) == 1 {
            for _ in 0..20_000 {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            }
            core::hint::black_box(x);
        } else {
            delay_ms(50);
        }
    }
}

// ---- The display ----
const BARS: [Color; 8] = [Color::WHITE, Color::YELLOW, Color::CYAN, Color::GREEN, Color::MAGENTA, Color::RED, Color::BLUE, Color::BLACK];

/// The background of a mode, painted into the rectangle `(x, y, w, h)` only.
fn background(s: &mut Surface, mode: u32, x: i32, y: i32, w: i32, h: i32) {
    let (x0, x1) = (x.max(0), (x + w).min(W as i32));
    match mode {
        1 => {
            let bw = W as i32 / 8;
            for bx in (x0 / bw)..=((x1 - 1) / bw).min(7) {
                let lo = (bx * bw).max(x0);
                let hi = ((bx + 1) * bw).min(x1);
                s.fill_rect(lo, y, hi - lo, h, BARS[bx as usize]);
            }
        }
        2 => {
            for cx in x0..x1 {
                s.vline(cx, y, h, Color::hue((cx * 255 / W as i32) as u8));
            }
        }
        _ => s.fill_rect(x0, y, x1 - x0, h, Color::rgb(8, 8, 40)),
    }
}

const PANEL: (i32, i32, i32, i32) = (0, 0, 760, 200);

fn panel(s: &mut Surface, mode: u32) {
    background(s, mode, PANEL.0, PANEL.1, PANEL.2, PANEL.3);
    s.fill_rect(PANEL.0 + 8, PANEL.1 + 8, PANEL.2 - 16, PANEL.3 - 16, Color::rgb(0, 0, 0));
    let ip = ADDRESS.load(Relaxed).to_be_bytes();
    let mut line = [0u8; 80];
    let rows = [
        format_args!("HARDROXIDE RTOS  https://{}.{}.{}.{}/", ip[0], ip[1], ip[2], ip[3]),
        format_args!("control loop: {} releases, worst {} us", RELEASES.load(Relaxed), WORST_US.load(Relaxed)),
        format_args!("audio: {} frames, {} underruns, vol {}", PUMP.serviced(), PUMP.underruns(), VOLUME.load(Relaxed)),
        format_args!("web: {} requests   display: {} frames", REQUESTS.load(Relaxed), FRAMES.load(Relaxed)),
    ];
    for (i, r) in rows.into_iter().enumerate() {
        let n = fmt(&mut line, r);
        s.text(PANEL.0 + 20, PANEL.1 + 20 + 40 * i as i32, core::str::from_utf8(&line[..n]).unwrap_or("?"), Color::WHITE, None, 2);
    }
}

fn display() {
    let (front, back) = unsafe {
        ps7::mem::set_non_cacheable(FB_BASE, 2 * FB_STRIDE);
        (core::slice::from_raw_parts_mut(FB_BASE as *mut u8, FRAME_BYTES), core::slice::from_raw_parts_mut((FB_BASE + FB_STRIDE) as *mut u8, FRAME_BYTES))
    };
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        let display = Display {
            dynclk: &d.video_hdmi_out_frontend_axi_dynclk,
            ref_clk_hz: 100_000_000,
            vtc: &d.video_hdmi_out_frontend_vtc_out,
            vdma: &d.video_axi_vdma,
            colour: d.video_hdmi_out_color_convert.regs(),
            unpack: d.video_hdmi_out_pixel_unpack.regs(),
        };
        for b in [&mut *front, &mut *back] {
            Surface::new(as_pixels(b).unwrap(), W, H).clear(Color::BLACK);
        }
        let hz = display.start(&MODE, &[FB_BASE as u32, (FB_BASE + FB_STRIDE) as u32]).unwrap();
        println!("display up, pixel clock {hz} Hz");
    }
    const R: i32 = 40;
    let (mut x, mut y, mut dx, mut dy) = (300i32, 400i32, 7i32, 5i32);
    let mut drawn = [(300i32, 400i32); 2];
    let (mut showing, mut mode_seen, mut panel_at) = (0u32, 0u32, 0u64);
    // The panel text changes twice a second; each buffer is repainted with it the next time it is hidden.
    let mut panel_dirty = [false; 2];
    let mut every = Periodic::every_ms(16);
    loop {
        every.wait_next();
        let t0 = Instant::now();
        let mode = MODE_SEL.load(Relaxed);
        let gen_now = MODE_GEN.load(Relaxed);
        if gen_now != mode_seen {
            // A new mode: repaint both buffers.
            mode_seen = gen_now;
            for b in [&mut *front, &mut *back] {
                let mut s = Surface::new(as_pixels(b).unwrap(), W, H);
                background(&mut s, mode, 0, 0, W as i32, H as i32);
                panel(&mut s, mode);
            }
            drawn = [(x, y), (x, y)];
        }
        let hidden = if showing == 0 { &mut *back } else { &mut *front };
        let idx = (showing ^ 1) as usize;
        let mut s = Surface::new(as_pixels(hidden).unwrap(), W, H);
        let now_ms = u64::from(FRAMES.load(Relaxed)) * 16;
        if now_ms >= panel_at {
            panel_dirty = [true, true];
            panel_at = now_ms + 500;
        }
        if panel_dirty[idx] {
            panel(&mut s, mode);
            panel_dirty[idx] = false;
        }
        let (ox, oy) = drawn[idx];
        background(&mut s, mode, ox - R - 2, oy - R - 2, 2 * R + 5, 2 * R + 5);
        s.fill_circle(x, y, R, Color::hue((FRAMES.load(Relaxed) * 3 % 256) as u8));
        s.circle(x, y, R + 1, Color::WHITE);
        drawn[idx] = (x, y);
        DRAW_WORST_US.fetch_max(t0.elapsed().as_micros() as u32, Relaxed);
        showing ^= 1;
        {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            let _ = d.video_axi_vdma.show_frame(showing);
        }
        FRAMES.fetch_add(1, Relaxed);
        x += dx;
        y += dy;
        if x < R + 2 || x > W as i32 - R - 2 {
            dx = -dx;
        }
        if y < PANEL.3 + R + 2 || y > H as i32 - R - 2 {
            dy = -dy;
        }
    }
}

/// A quarter-size copy of the screen as a 24-bit BMP, produced in pieces.
fn screen_bmp(_arg: u32, offset: usize, out: &mut [u8]) -> usize {
    const SW: usize = W / 4;
    const SH: usize = H / 4;
    let header = header_24bit(SW as u32, SH as u32);
    let row_bytes = (SW * 3).next_multiple_of(4);
    // The first frame buffer (both carry the same picture but for the ball, and a picture is cut from one of them, so it is consistent).
    let pixels = unsafe { core::slice::from_raw_parts(FB_BASE as *const u32, FRAME_BYTES / 4) };
    for (i, o) in out.iter_mut().enumerate() {
        let at = offset + i;
        *o = if at < 54 {
            header[at]
        } else {
            let p = at - 54;
            let (row, col) = (p / row_bytes, p % row_bytes);
            let y = (SH - 1 - row) * 4;
            if col / 3 < SW { (pixels[y * W + (col / 3) * 4] >> (8 * (col % 3))) as u8 } else { 0 }
        };
    }
    out.len()
}

// ---- The web ----
const PAGE: &[u8] = b"<!doctype html><html><head><meta charset=utf-8><meta name=viewport content='width=device-width'><title>Hardroxide station</title>\
<style>body{font-family:sans-serif;margin:1.5em;max-width:46em;background:#111;color:#eee}button{font-size:1.1em;margin:.2em;padding:.4em .8em}.on{background:#e33;color:#fff}\
pre{background:#222;padding:.6em;overflow:auto}h1{font-size:1.4em}td,th{padding:.1em .6em;text-align:right}img{width:640px;max-width:100%;image-rendering:pixelated}</style></head><body>\
<h1>Hardroxide station: HTTPS, graphics, audio and a 10 ms control loop on one core</h1>\
<p>LEDs: <span id=leds></span></p><p>Audio: <button onclick=\"c('/audio?cmd=play')\">play</button><button onclick=\"c('/audio?cmd=stop')\">stop</button><button onclick=\"c('/audio?cmd=next')\">next file</button>\
<button onclick=\"c('/volume?v=30')\">quiet</button><button onclick=\"c('/volume?v=50')\">normal</button></p>\
<p>Display: <button onclick=\"c('/display?mode=0')\">ball</button><button onclick=\"c('/display?mode=1')\">bars</button><button onclick=\"c('/display?mode=2')\">rainbow</button>\
 Load: <button onclick=\"c('/load?on=1')\">burn CPU</button><button onclick=\"c('/load?on=0')\">idle</button></p>\
<table id=tasks></table><pre id=s>loading...</pre><img id=pic src=/screen.bmp alt=screen>\
<script>let v=0;function c(u){fetch(u).then(r=>r.text()).then(refresh)}function led(n){v^=1<<n;c('/led?v='+v)}\
function refresh(){fetch('/api/status').then(r=>r.json()).then(j=>{v=j.leds;\
document.getElementById('leds').innerHTML=[0,1,2,3].map(n=>\"<button class='\"+((j.leds>>n)&1?'on':'')+\"' onclick='led(\"+n+\")'>\"+n+'</button>').join('');\
document.getElementById('tasks').innerHTML='<tr><th>task<th>prio<th>cpu %<th>stack</tr>'+j.tasks.map(t=>'<tr><td>'+t.name+'<td>'+t.prio+'<td>'+(t.cpu_permille/10).toFixed(1)+'<td>'+t.stack_used+'/'+t.stack_size+'</tr>').join('')+'<tr><td>interrupts<td><td>'+(j.irq_permille/10).toFixed(1)+'<td></tr>';\
document.getElementById('s').textContent=JSON.stringify(j,(k,x)=>k==='tasks'?undefined:x,1);document.getElementById('pic').src='/screen.bmp?'+Date.now()})}\
refresh();setInterval(refresh,2000)</script></body></html>";

fn network() {
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    unsafe { ps7::mem::set_non_cacheable(DMA_BASE, DMA_REGION_BYTES) };
    let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, [0x02, 0x48, 0x52, 0x58, 0x00, 0x01]) };
    gem.init().unwrap();
    if let Err(e) = gem.link_up(&slcr, 15_000) {
        println!("no link: {e}");
        return;
    }
    // The card: sound files, the random seed, and the codec for volume changes.
    let sdbase = ps7::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32;
    let mut sd = SdHost::new(be.map(ps7::map::SDIO0_BASE as u64, ps7::map::SDIO_SIZE).unwrap(), sdbase);
    let mut drbg = Drbg::new(b"hardroxide rt_station");
    drbg.add_entropy(&ps7::time::now_ns().to_le_bytes());
    let mut files = [[0u8; 32]; 8];
    let mut n_files = 0;
    let mut may_write = false;
    let mut vol = if let Ok(info) = sd.init() {
        SD_MIB.store((info.blocks / 2048) as u32, Relaxed);
        may_write = info.serial_matches(option_env!("HRX_WRITE_SERIAL"));
        Volume::mount(sd).ok()
    } else {
        None
    };
    if let Some(v) = vol.as_mut() {
        if let Ok(mut f) = v.open("/RNGSEED", OpenMode::Read) {
            let mut old = [0u8; 64];
            let n = f.read(v, &mut old).unwrap_or(0);
            drbg.add_entropy(&old[..n]);
        }
        // The only write, and only to the card named when building.
        if may_write {
            let mut fresh = [0u8; 32];
            drbg.fill(&mut fresh);
            if let Ok(mut f) = v.open("/RNGSEED", OpenMode::Write) {
                let _ = f.write(v, &fresh);
                let _ = f.close(v);
                let _ = v.sync();
            }
        }
        let _ = v.read_dir("/", |e| {
            let name = e.name.as_str();
            if !e.is_dir && n_files < files.len() && name.len() < 32 && (name.ends_with(".wav") || name.ends_with(".WAV")) && e.size as usize <= WAV_MAX {
                files[n_files][..name.len()].copy_from_slice(name.as_bytes());
                n_files += 1;
            }
        });
    }
    println!("card: {} MiB, {} sound file(s)", SD_MIB.load(Relaxed), n_files);
    let i2c = ps7::i2c::I2c::new(be.map(ps7::map::I2C1_BASE as u64, ps7::map::I2C_SIZE).unwrap());
    let mut codec = Adau1761::new(i2c);

    let config = Config::Dhcp { fallback: Some(StaticConfig { ip: [192, 168, 2, 100], netmask: [255, 255, 255, 0], gateway: Some([192, 168, 2, 1]) }), fallback_after_ms: 8000 };
    let t0 = Instant::now();
    let ms = || t0.elapsed().as_millis() as u64;
    let mut net: Stack<_, 4, 1, 2048> = Stack::new(gem, config, ms());
    let tls_config = TlsConfig::p256(CERT, *KEY);
    let mut random = || {
        drbg.add_entropy(&ps7::time::now_ns().to_le_bytes());
        drbg.bytes64()
    };
    let mut server: HttpsServer<4, 1024, 2048> = HttpsServer::new(443, 0, &tls_config, &mut random);
    let mut current_file = 0usize;
    let mut announced = false;
    loop {
        net.poll(ms());
        if net.is_configured() && !announced {
            announced = true;
            ADDRESS.store(u32::from_be_bytes(net.ip()), Relaxed);
            let ip = net.ip();
            println!("STATION READY: https://{}.{}.{}.{}/", ip[0], ip[1], ip[2], ip[3]);
        }
        let stats = net.stats();
        let ip = net.ip();
        server.poll(&mut net, &mut random, |req, res| {
            REQUESTS.fetch_add(1, Relaxed);
            match (req.method, req.path) {
                ("GET", "/") => res.bytes(200, "text/html; charset=utf-8", PAGE),
                ("GET", "/led") => match req.param("v").and_then(|v| v.parse::<u32>().ok()) {
                    Some(v) if v < 16 => {
                        LEDS.store(v, Relaxed);
                        if let Some(d) = DESIGN.lock(100) {
                            let _ = d.leds_gpio.write(Channel::One, v);
                        }
                        res.text(200, "text/plain", format_args!("LEDs {v:04b}\n"));
                    }
                    _ => res.text(400, "text/plain", format_args!("use /led?v=N with N from 0 to 15\n")),
                },
                ("GET", "/audio") => {
                    match req.param("cmd") {
                        Some("play") => AUDIO_PLAYING.store(1, Relaxed),
                        Some("stop") => AUDIO_PLAYING.store(0, Relaxed),
                        Some("next") if n_files > 0 => {
                            // Load the next sound file from the card into the buffer; the feeder lets go of it while it is rewritten.
                            current_file = (current_file + 1) % n_files;
                            let name = core::str::from_utf8(&files[current_file]).unwrap_or("").trim_end_matches('\0');
                            let mut path = [0u8; 40];
                            let pl = fmt(&mut path, format_args!("/{name}"));
                            WAV_LEN.store(0, Relaxed);
                            AUDIO_GEN.fetch_add(1, Relaxed);
                            delay_ms(20);
                            if let Some(v) = vol.as_mut()
                                && let Ok(mut f) = v.open(core::str::from_utf8(&path[..pl]).unwrap_or("/"), OpenMode::Read)
                            {
                                // SAFETY: the feeder is not reading WAV_BUF (WAV_LEN is 0); only this task writes it.
                                let buf = unsafe { &mut *core::ptr::addr_of_mut!(WAV_BUF) };
                                let size = f.len() as usize;
                                let mut at = 0;
                                while at < size.min(WAV_MAX) {
                                    match f.read(v, &mut buf[at..size.min(WAV_MAX)]) {
                                        Ok(0) | Err(_) => break,
                                        Ok(k) => at += k,
                                    }
                                }
                                if Wav::parse(&buf[..at]).is_ok() {
                                    WAV_LEN.store(at as u32, Relaxed);
                                }
                            }
                            AUDIO_GEN.fetch_add(1, Relaxed);
                        }
                        _ => {}
                    }
                    res.text(200, "text/plain", format_args!("audio {}\n", if AUDIO_PLAYING.load(Relaxed) == 1 { "playing" } else { "stopped" }));
                }
                ("GET", "/volume") => match req.param("v").and_then(|v| v.parse::<u32>().ok()) {
                    Some(v) if v <= 62 => {
                        VOLUME.store(v, Relaxed);
                        let _ = codec.enable_playback(v as u8);
                        res.text(200, "text/plain", format_args!("volume {v}\n"));
                    }
                    _ => res.text(400, "text/plain", format_args!("use /volume?v=N with N from 0 to 62\n")),
                },
                ("GET", "/display") => match req.param("mode").and_then(|v| v.parse::<u32>().ok()) {
                    Some(m) if m < 3 => {
                        MODE_SEL.store(m, Relaxed);
                        MODE_GEN.fetch_add(1, Relaxed);
                        res.text(200, "text/plain", format_args!("display mode {m}\n"));
                    }
                    _ => res.text(400, "text/plain", format_args!("use /display?mode=N with N from 0 to 2\n")),
                },
                ("GET", "/load") => {
                    LOAD_ON.store(u32::from(req.param("on") == Some("1")), Relaxed);
                    res.text(200, "text/plain", format_args!("load {}\n", if LOAD_ON.load(Relaxed) == 1 { "on" } else { "off" }));
                }
                ("GET", "/screen.bmp") => res.generated(200, "image/bmp", 54 + (W / 4 * 3).next_multiple_of(4) * (H / 4), screen_bmp, 0),
                ("GET", "/api/status") => {
                    let buf = res.scratch();
                    let mut w = Cursor { buf, len: 0 };
                    let total = cpu_total_ticks().max(1);
                    let (sw, btn) = DESIGN.lock(100).map(|d| (d.switches_gpio.read(Channel::One).unwrap_or(0) & 3, d.btns_gpio.read(Channel::One).unwrap_or(0) & 15)).unwrap_or((0, 0));
                    let _ = core::fmt::Write::write_fmt(
                        &mut w,
                        format_args!(
                            "{{\"ip\":\"{}.{}.{}.{}\",\"uptime_ms\":{},\"leds\":{},\"switches\":{sw},\"buttons\":{btn},\"control_releases\":{},\"control_worst_deviation_us\":{},\
\"audio\":{{\"playing\":{},\"frames\":{},\"underruns\":{},\"volume\":{},\"file\":{}}},\"display\":{{\"frames\":{},\"mode\":{},\"draw_worst_us\":{}}},\
\"requests\":{},\"rx_frames\":{},\"tx_frames\":{},\"sd_mib\":{},\"load\":{},\"irq_permille\":{},\"tasks\":[",
                            ip[0], ip[1], ip[2], ip[3], t0.elapsed().as_millis(), LEDS.load(Relaxed), RELEASES.load(Relaxed), WORST_US.load(Relaxed),
                            AUDIO_PLAYING.load(Relaxed), PUMP.serviced(), PUMP.underruns(), VOLUME.load(Relaxed), if WAV_LEN.load(Relaxed) == 0 { 0 } else { current_file + 1 },
                            FRAMES.load(Relaxed), MODE_SEL.load(Relaxed), DRAW_WORST_US.load(Relaxed), REQUESTS.load(Relaxed), stats.rx_frames, stats.tx_frames, SD_MIB.load(Relaxed),
                            LOAD_ON.load(Relaxed), irq_ticks() * 1000 / total
                        ),
                    );
                    let mut first = true;
                    for slot in 0..task_slots() {
                        if let Some(t) = task_info(slot) {
                            let _ = core::fmt::Write::write_fmt(
                                &mut w,
                                format_args!("{}{{\"name\":\"{}\",\"prio\":{},\"cpu_permille\":{},\"stack_used\":{},\"stack_size\":{}}}", if first { "" } else { "," }, t.name, t.priority, t.run_ticks * 1000 / total, t.stack_used, t.stack_size),
                            );
                            first = false;
                        }
                    }
                    let _ = core::fmt::Write::write_str(&mut w, "]}\n");
                    let n = w.len;
                    res.scratch_body(200, "application/json", n);
                }
                _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
            }
        });
        // Give the processor back between polls: the stack needs one every millisecond, not every microsecond.
        delay_ms(1);
    }
}

struct Cursor<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl core::fmt::Write for Cursor<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        if n < s.len() { Err(core::fmt::Error) } else { Ok(()) }
    }
}

fn fmt(buf: &mut [u8], args: core::fmt::Arguments) -> usize {
    let mut c = Cursor { buf, len: 0 };
    let _ = core::fmt::Write::write_fmt(&mut c, args);
    c.len
}

fn report() {
    loop {
        delay_ms(5000);
        let total = cpu_total_ticks().max(1);
        println!(
            "control: {} releases, worst {} us | display: {} frames, draw worst {} us | audio: {} frames, {} underruns | web: {} requests | irq {}.{}%",
            RELEASES.load(Relaxed),
            WORST_US.load(Relaxed),
            FRAMES.load(Relaxed),
            DRAW_WORST_US.load(Relaxed),
            PUMP.serviced(),
            PUMP.underruns(),
            REQUESTS.load(Relaxed),
            irq_ticks() * 100 / total,
            irq_ticks() * 1000 / total % 10
        );
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
    println!("\n== rt_station: HTTPS, graphics, audio and a 10 ms control loop ==");
    // The codec, over the PS I2C, before the kernel starts.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let i2c = ps7::i2c::I2c::new(be.map(ps7::map::I2C1_BASE as u64, ps7::map::I2C_SIZE).unwrap());
    i2c.set_clock(ps7::clocks::cpu_1x_hz(&slcr, 50_000_000).unwrap() as u32, 400_000).unwrap();
    let mut codec = Adau1761::new(i2c);
    if !codec.init().unwrap_or(false) {
        println!("codec bring-up failed");
        halt()
    }
    codec.enable_playback(VOLUME.load(Relaxed) as u8).unwrap();
    d.leds_gpio.set_input_mask(Channel::One, 0).unwrap();
    DESIGN.set(d);
    spawn("control", 6, task_stack!(8192).unwrap(), control).unwrap();
    spawn("feed", 4, task_stack!(8192).unwrap(), feed).unwrap();
    spawn("display", 3, task_stack!(65536).unwrap(), display).unwrap();
    spawn("network", 3, task_stack!(262144).unwrap(), network).unwrap();
    spawn("hog", 1, task_stack!(8192).unwrap(), hog).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
