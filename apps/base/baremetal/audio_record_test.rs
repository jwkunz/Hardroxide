//! Base overlay, bare metal: test recording and playback from memory with one cable, HPH OUT to LINE IN.
//!
//! 1. Play a 1 kHz tone and record it from the input in the same loop into a buffer (`play_record`).
//! 2. Play that recording back out of the headphone jack while recording again into a second buffer.
//! Each recording is checked for a 1 kHz tone (rising zero crossings), so it proves that what was recorded
//! into memory is what was played, and that playing from memory works. Prints `RECORD/PLAYBACK OK`.
//! Needs `hardroxide-audio`. Base overlay application 12.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_audio::adau1761::{Adau1761, Input};
use hardroxide_audio::io::play_record;
use hardroxide_audio::tone::Oscillator;
use hardroxide_audio::{Frame, SAMPLE_RATE_HZ};
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;

ps7::entry!(main);

const SECONDS: usize = 2;
const FRAMES: usize = 48_828 * SECONDS;
static mut FIRST: [Frame; FRAMES] = [Frame { left: 0, right: 0 }; FRAMES];
static mut SECOND: [Frame; FRAMES] = [Frame { left: 0, right: 0 }; FRAMES];

/// `(rising zero crossings per second, peak in percent of full scale)` of the left channel, skipping the first
/// half second (the converters and the codec's filters settle).
fn analyse(buf: &[Frame]) -> (f32, f32) {
    let skip = buf.len() / 4;
    let data = &buf[skip..];
    let mean = data.iter().map(|f| i64::from(f.left)).sum::<i64>() / data.len() as i64;
    let (mut crossings, mut last, mut peak) = (0u32, 0i32, 0i32);
    for f in data {
        let v = f.left - mean as i32;
        peak = peak.max(v.abs());
        if last < -(1 << 12) && v >= 0 {
            crossings += 1;
            last = v;
        } else if v.abs() > (1 << 12) || last >= 0 && v < -(1 << 12) {
            last = v;
        }
    }
    let secs = data.len() as f32 / SAMPLE_RATE_HZ as f32;
    (crossings as f32 / secs, peak as f32 * 100.0 / (1 << 23) as f32)
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
    i2c.set_clock(ps7::clocks::cpu_1x_hz(&slcr, 50_000_000).unwrap() as u32, 400_000).unwrap();
    let mut codec = Adau1761::new(i2c);
    if !codec.init().unwrap_or(false) {
        println!("codec bring-up failed");
        halt()
    }
    codec.select_input(Input::LineIn).unwrap();
    codec.enable_playback(40).unwrap();
    let audio = &d.audio_codec_ctrl_0;
    println!("\n== audio_record_test: HPH OUT cabled to LINE IN ==");

    // SAFETY: nothing else touches the buffers.
    let (first, second): (&mut [Frame], &mut [Frame]) = unsafe { (&mut *core::ptr::addr_of_mut!(FIRST), &mut *core::ptr::addr_of_mut!(SECOND)) };

    let mut osc = Oscillator::new(1000.0, SAMPLE_RATE_HZ as f32);
    let tone = core::iter::from_fn(|| {
        let s = i32::from(osc.next_sample()) << 7;
        Some(Frame { left: s, right: s })
    });
    let st = play_record(audio, tone, first).unwrap();
    let (hz1, peak1) = analyse(first);
    println!("1. tone played and recorded: {} frames, {} missed; recording holds {:.0} Hz at {:.1} % peak", st.frames, st.missed, hz1, peak1);

    let st = play_record(audio, first.iter().copied(), second).unwrap();
    let (hz2, peak2) = analyse(second);
    println!("2. recording played back and recorded again: {} frames, {} missed; {:.0} Hz at {:.1} % peak", st.frames, st.missed, hz2, peak2);

    codec.mute().unwrap();
    let ok = (950.0..1050.0).contains(&hz1) && (950.0..1050.0).contains(&hz2) && peak1 > 2.0 && peak2 > 2.0;
    println!("{}", if ok { "RECORD/PLAYBACK OK" } else { "RECORD/PLAYBACK FAILED" });
    halt()
}
