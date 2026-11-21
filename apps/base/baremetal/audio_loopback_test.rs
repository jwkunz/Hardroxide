//! Base overlay, bare metal: an audio self-test with a cable from the headphone output to the input.
//!
//! Plays a 1 kHz sine out of the headphone jack while recording the input in the same sample loop (the
//! controller is full duplex), first one second of silence for the noise floor, then two seconds of tone (the
//! first half second of each is skipped while the converters settle). For
//! the tone it prints the peak and RMS level of each channel and the frequency found by counting
//! rising zero crossings, and `LOOPBACK OK` if a 1 kHz tone came back. Connect the green HPH OUT jack to the
//! blue LINE IN jack with a stereo cable (the pink MIC jack is a differential microphone input and
//! did not hear the tone in testing).
//! Needs `hardroxide-audio`. Base overlay application 11.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_audio::adau1761::{Adau1761, Input};
use hardroxide_audio::tone::Oscillator;
use hardroxide_audio::{Frame, SAMPLE_RATE_HZ};
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;

ps7::entry!(main);

const INPUT: Input = Input::LineIn;
/// Headphone volume 0 (-57 dB) to 62 (+5 dB).
const VOLUME: u8 = 40;

#[derive(Default)]
struct Stats {
    n: u32,
    peak: i32,
    sum: i64,
    sum_sq: i64,
    crossings: u32,
    last: i32,
}

impl Stats {
    fn add(&mut self, v: i32) {
        self.n += 1;
        self.peak = self.peak.max(v.abs());
        self.sum += i64::from(v);
        self.sum_sq += i64::from(v >> 4) * i64::from(v >> 4);
        // Rising zero crossings of the DC-removed signal, with a little hysteresis against noise.
        let centred = v - (self.sum / i64::from(self.n)) as i32;
        if self.last < -(1 << 12) && centred >= 0 {
            self.crossings += 1;
            self.last = centred;
        } else if centred.abs() > (1 << 12) || self.last >= 0 && centred < -(1 << 12) {
            self.last = centred;
        }
    }
    fn rms_percent(&self) -> f32 {
        let mean_square = (self.sum_sq / i64::from(self.n.max(1))) as u64;
        // Integer square root (no floating-point `sqrt` without an operating system's math library).
        let (mut lo, mut hi) = (0u64, 1u64 << 32);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if mid * mid <= mean_square { lo = mid } else { hi = mid - 1 }
        }
        let full = (1 << 19) as f32; // 24-bit full scale, in the >> 4 units used for the sum
        lo as f32 * 100.0 / full
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
    // SAFETY: the SLCR is read here; the I2C 1 window is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let i2c = ps7::i2c::I2c::new(be.map(ps7::map::I2C1_BASE as u64, ps7::map::I2C_SIZE).unwrap());
    let in_hz = ps7::clocks::cpu_1x_hz(&slcr, 50_000_000).unwrap() as u32;
    i2c.set_clock(in_hz, 400_000).unwrap();
    let mut codec = Adau1761::new(i2c);
    if !codec.init().unwrap_or(false) {
        println!("codec bring-up failed");
        halt()
    }
    codec.select_input(INPUT).unwrap();
    codec.enable_playback(VOLUME).unwrap();
    let audio = &d.audio_codec_ctrl_0;
    println!("\n== audio_loopback_test: HPH OUT cabled to the input ==");

    let rate = SAMPLE_RATE_HZ as f32;
    let mut osc = Oscillator::new(1000.0, rate);
    let (mut quiet, mut loud) = ([Stats::default(), Stats::default()], [Stats::default(), Stats::default()]);
    let frames = (rate as u32) * 3;
    audio.clear_ready().unwrap();
    let mut missed = 0u32;
    for i in 0..frames {
        if audio.ready().unwrap() && i > 0 {
            missed += 1;
        }
        if !audio.wait_sample(2_000_000).unwrap() {
            println!("no sample clock");
            halt()
        }
        let rx = audio.read_frame().unwrap();
        let tone = i >= rate as u32; // the first second is silence
        let s = if tone { i32::from(osc.next_sample()) << 7 } else { 0 }; // about 25 % of full scale
        audio.write_frame(Frame { left: s, right: s }).unwrap();
        // Skip the first half second of each phase: the converters settle and the tone's start is a transient.
        let phase_start = if tone { rate as u32 } else { 0 };
        if i - phase_start >= (rate as u32) / 2 {
            let st = if tone { &mut loud } else { &mut quiet };
            st[0].add(rx.left);
            st[1].add(rx.right);
        }
    }
    codec.mute().unwrap();
    println!("{missed} samples missed in {frames} frames");
    let pct = |p: i32| p as f32 * 100.0 / (1 << 23) as f32;
    for (name, q, l) in [("left", &quiet[0], &loud[0]), ("right", &quiet[1], &loud[1])] {
        let secs = l.n as f32 / rate;
        println!(
            "{name}: silence peak {:.2} %, tone peak {:.1} % rms {:.1} %, {:.0} Hz ({} crossings in {:.2} s)",
            pct(q.peak),
            pct(l.peak),
            l.rms_percent(),
            l.crossings as f32 / secs,
            l.crossings,
            secs
        );
    }
    let ok = [&loud[0], &loud[1]].iter().any(|l| {
        let hz = l.crossings as f32 / (l.n as f32 / rate);
        (900.0..1100.0).contains(&hz) && pct(l.peak) > 2.0
    });
    println!("{}", if ok { "LOOPBACK OK" } else { "LOOPBACK FAILED" });
    halt()
}
