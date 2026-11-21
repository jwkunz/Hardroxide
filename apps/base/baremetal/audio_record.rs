//! Base overlay, bare metal: record from the line input, then play it back.
//!
//! Records five seconds of stereo audio from the **LINE IN** (blue) jack into memory (a 2 MiB
//! buffer: 48.8 kHz x 5 s x 8 bytes), prints the peak level of each channel, then plays it back on the
//! headphone output. Feed it a source (a phone, a computer) before it starts: it begins recording a
//! few seconds after the codec is up. Needs `hardroxide-audio`. Base overlay application 9.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_audio::adau1761::{Adau1761, Input};
use hardroxide_audio::io::{play, record};
use hardroxide_audio::Frame;
use hardroxide_mmio::time::sleep;
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;

ps7::entry!(main);

const SECONDS: usize = 5;
const FRAMES: usize = 48_828 * SECONDS;
static mut BUFFER: [Frame; FRAMES] = [Frame { left: 0, right: 0 }; FRAMES];

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
    codec.select_input(Input::LineIn).unwrap();
    let audio = &d.audio_codec_ctrl_0;
    println!("\n== audio_record: {SECONDS} s from the line input, then playback ==");
    println!("recording starts in 3 s");
    sleep(Duration::from_secs(3));

    // SAFETY: nothing else touches `BUFFER`.
    let buf: &mut [Frame] = unsafe { &mut *core::ptr::addr_of_mut!(BUFFER) };
    let st = record(audio, buf).unwrap();
    println!("recorded {} frames, {} missed", st.frames, st.missed);
    let (mut pl, mut pr) = (0i32, 0i32);
    for f in buf.iter() {
        pl = pl.max(f.left.abs());
        pr = pr.max(f.right.abs());
    }
    println!("peak level: left {:.1} % right {:.1} % of full scale", pl as f32 * 100.0 / (1 << 23) as f32, pr as f32 * 100.0 / (1 << 23) as f32);

    codec.select_input(Input::None).unwrap();
    codec.enable_playback(50).unwrap();
    let st = play(audio, buf.iter().copied()).unwrap();
    println!("played {} frames, {} missed", st.frames, st.missed);
    codec.mute().unwrap();
    println!("done");
    halt()
}
