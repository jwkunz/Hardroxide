//! Base overlay, RTOS: audio playback by interrupt, beside a 10 ms control loop and a processor hog.
//!
//! The audio controller has no FIFO: a frame must be written every 20.5 us, which no task can do beside other work. Here a timer
//! interrupt does it: the global timer's comparator raises an interrupt at twice the sample rate (97.656 kHz); its handler asks the
//! controller whether a sample tick has come and, if so, moves the next frame from a lock-free ring to it. A task (`feed`, priority 4)
//! keeps the ring full from the built-in `chime.wav` (resampled to the codec's rate, looped). Every 5 seconds the report shows how many
//! frames the handler wrote against how many sample periods the processor's clock counted (the ratio is the codec's clock against the processor's, a few parts per million;
//! a missed sample would show as a jump), the underruns (ticks that found the ring empty), each task's share of the processor, the share spent in interrupt handlers, and the
//! control loop's worst deviation. Copy `apps/base/data/chime.wav` to the crate's `examples/` first; plug headphones into
//! **HPH OUT**. Needs `hardroxide-audio`. Base overlay application 24.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{Periodic, cpu_total_ticks, delay_ms, irq_ticks, spawn, start, task_info, task_slots, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_audio::SAMPLE_RATE_HZ;
use hardroxide_audio::adau1761::Adau1761;
use hardroxide_audio::pump::Pump;
use hardroxide_audio::resample::Resample;
use hardroxide_audio::wav::Wav;
use hardroxide_ip::audio::AudioCodecCtrl;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use ps7::gic::Gic;
use ps7::println;
use ps7::timer::GlobalTimer;

ps7::entry!(main);

static CHIME: &[u8] = include_bytes!("chime.wav");
static PUMP: Pump<4096> = Pump::new();
static WORST_US: AtomicU32 = AtomicU32::new(0);
static RELEASES: AtomicU32 = AtomicU32::new(0);

struct IsrState {
    ctrl: AudioCodecCtrl<PhysRegion>,
    timer: GlobalTimer<PhysRegion>,
}

static mut ISR: Option<IsrState> = None;

/// The global timer interrupt: one call per half sample period.
fn audio_isr() {
    // SAFETY: ISR is written once, before the interrupt is enabled, and only read afterwards.
    if let Some(st) = unsafe { (*core::ptr::addr_of!(ISR)).as_ref() } {
        let _ = st.timer.clear_event();
        PUMP.service(&st.ctrl);
    }
}

/// Keep the ring full; start the interrupt first.
fn feed() {
    // SAFETY: the GIC window is shared with the kernel, which only touches it with interrupts masked; the global timer
    // belongs to the interrupt handler from here on.
    let be = unsafe { PhysMem::new() };
    let gic = Gic::new(
        be.map(ps7::map::GIC_DIST_BASE as u64, ps7::map::GIC_DIST_SIZE).unwrap(),
        be.map(ps7::map::GIC_CPU_BASE as u64, ps7::map::GIC_CPU_SIZE).unwrap(),
    );
    let timer = GlobalTimer::new(be.map(ps7::map::GLOBAL_TIMER_BASE as u64, ps7::map::GLOBAL_TIMER_SIZE).unwrap());
    let ctrl = AudioCodecCtrl::new(be.map(design_1::pac::audio_codec_ctrl_0::BASE_ADDRESS, design_1::pac::audio_codec_ctrl_0::SIZE).unwrap());
    // Preload the ring so the first ticks have something to play.
    let wav = Wav::parse(CHIME).unwrap();
    let mut frames = Resample::new(wav.frames(), wav.sample_rate, SAMPLE_RATE_HZ as u32);
    while PUMP.space() > 0 {
        let f = frames.next().unwrap_or_default();
        PUMP.push(f);
    }
    // SAFETY: nothing else touches ISR before the interrupt is enabled below.
    unsafe { *core::ptr::addr_of_mut!(ISR) = Some(IsrState { ctrl, timer: GlobalTimer::new(be.map(ps7::map::GLOBAL_TIMER_BASE as u64, ps7::map::GLOBAL_TIMER_SIZE).unwrap()) }) };
    ps7::irq::register(ps7::map::ppi::GLOBAL_TIMER, audio_isr);
    gic.set_priority(ps7::map::ppi::GLOBAL_TIMER, 0x40).unwrap();
    gic.enable(ps7::map::ppi::GLOBAL_TIMER).unwrap();
    let periph = design_1::periph_hz().unwrap();
    let period = (periph as f64 / (2.0 * SAMPLE_RATE_HZ)) as u32; // 3328 ticks at 325 MHz
    timer.start_periodic_interrupt(period).unwrap();
    println!("audio pump running: timer interrupt every {period} ticks ({} Hz)", periph / u64::from(period));
    loop {
        while PUMP.space() > 0 {
            let f = match frames.next() {
                Some(f) => f,
                None => {
                    frames = Resample::new(wav.frames(), wav.sample_rate, SAMPLE_RATE_HZ as u32);
                    frames.next().unwrap_or_default()
                }
            };
            PUMP.push(f);
        }
        delay_ms(5);
    }
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

/// A processor hog below the audio feeder: it uses whatever time is left.
fn hog() {
    let mut x = 1u32;
    loop {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        core::hint::black_box(x);
    }
}

fn report() {
    let t0 = Instant::now();
    loop {
        delay_ms(5000);
        let elapsed_us = t0.elapsed().as_micros() as u64;
        let expected = (elapsed_us as f64 * SAMPLE_RATE_HZ / 1e6) as u32;
        let (done, under) = (PUMP.serviced(), PUMP.underruns());
        // The frames written against the sample periods the processor's clock counted: their ratio is the codec's clock against the processor's
        // (a few parts per million off), not missed samples; a real miss would show as a jump.
        let ppm = (i64::from(done) - i64::from(expected)) * 1_000_000 / i64::from(expected.max(1));
        println!("audio: {done} frames written, {expected} sample periods by the processor's clock ({ppm} ppm), {under} underruns");
        let total = cpu_total_ticks().max(1);
        for slot in 0..task_slots() {
            if let Some(t) = task_info(slot) {
                println!("  task {:<8} prio {} {:>5.1}% of the processor, stack {}/{} bytes", t.name, t.priority, t.run_ticks as f64 * 100.0 / total as f64, t.stack_used, t.stack_size);
            }
        }
        println!("  interrupts (the audio pump, the tick):   {:>5.1}%", irq_ticks() as f64 * 100.0 / total as f64);
        println!("  control loop: {} releases, worst deviation from 10 ms {} us", RELEASES.load(Relaxed), WORST_US.load(Relaxed));
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
    println!("\n== rt_audio: playback by interrupt ==");
    // The codec, over the PS I2C (before the kernel starts).
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let i2c = ps7::i2c::I2c::new(be.map(ps7::map::I2C1_BASE as u64, ps7::map::I2C_SIZE).unwrap());
    i2c.set_clock(ps7::clocks::cpu_1x_hz(&slcr, 50_000_000).unwrap() as u32, 400_000).unwrap();
    let mut codec = Adau1761::new(i2c);
    if !codec.init().unwrap_or(false) {
        println!("codec bring-up failed");
        halt()
    }
    codec.enable_playback(50).unwrap();
    drop(d);
    spawn("control", 6, task_stack!(8192).unwrap(), control).unwrap();
    spawn("feed", 4, task_stack!(8192).unwrap(), feed).unwrap();
    spawn("hog", 1, task_stack!(8192).unwrap(), hog).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
