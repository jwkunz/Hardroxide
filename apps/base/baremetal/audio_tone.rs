//! Base overlay, bare metal: play a tune on the headphone output.
//!
//! Brings up the codec over the PS's I2C 1 (routed to the codec's control pins through the EMIO),
//! then plays a scale and a chord as sine notes generated sample by sample at the codec's
//! 48.8 kHz. Plug headphones into the **HPH OUT** (green) jack. Needs `hardroxide-audio`.
//! Base overlay application 7.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_audio::adau1761::Adau1761;
use hardroxide_audio::io::{mix, play};
use hardroxide_audio::tone::Note;
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;

ps7::entry!(main);

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
    println!("\n== audio_tone ==");
    println!("I2C clock {} Hz", i2c.set_clock(in_hz, 400_000).unwrap());

    let mut codec = Adau1761::new(i2c);
    match codec.init() {
        Ok(true) => println!("codec up (PLL locked)"),
        Ok(false) => {
            println!("the codec's PLL did not lock");
            halt()
        }
        Err(e) => {
            println!("codec I2C error: {e}");
            halt()
        }
    }
    codec.enable_playback(50).unwrap();
    let audio = &d.audio_codec_ctrl_0;

    // A C major scale, then the chord.
    for midi in [60u8, 62, 64, 65, 67, 69, 71, 72] {
        let st = play(audio, Note::at_codec_rate(midi, 250)).unwrap();
        println!("note {midi}: {} frames, {} missed", st.frames, st.missed);
    }
    let chord = |m: u8| Note::at_codec_rate(m, 1200);
    let notes = chord(60).zip(chord(64)).zip(chord(67)).map(|((a, b), c)| mix(mix(a, b), c));
    let st = play(audio, notes.map(|f| hardroxide_audio::io::scale(f, 90))).unwrap();
    println!("chord: {} frames, {} missed", st.frames, st.missed);
    codec.mute().unwrap();
    println!("done");
    halt()
}
