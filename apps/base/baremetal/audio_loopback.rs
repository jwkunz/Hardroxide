//! Base overlay, bare metal: line in to headphone out, through the processor.
//!
//! The codec's input is digitised, the processor reads each frame from the I2S controller, halves
//! the volume, and writes it back out: a 30-second pass-through that takes one sample every
//! 20.5 us. The count of missed samples says whether the loop kept up. Needs `hardroxide-audio`.
//! Base overlay application 10.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_audio::adau1761::{Adau1761, Input};
use hardroxide_audio::io::{pass_through, scale};
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
    i2c.set_clock(in_hz, 400_000).unwrap();
    let mut codec = Adau1761::new(i2c);
    if !codec.init().unwrap_or(false) {
        println!("codec bring-up failed");
        halt()
    }
    codec.select_input(Input::LineIn).unwrap();
    codec.enable_playback(50).unwrap();
    println!("\n== audio_loopback: line in to headphones for 30 s ==");
    let st = pass_through(&d.audio_codec_ctrl_0, 48_828 * 30, |f| scale(f, 128)).unwrap();
    println!("{} frames passed through, {} missed", st.frames, st.missed);
    codec.mute().unwrap();
    halt()
}
