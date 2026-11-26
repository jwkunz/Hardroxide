//! Base overlay, bare metal: play a WAV file from the SD card.
//!
//! The card must carry a FAT32 file system (`sd_erase_and_test` makes one). If it has no `/chime.wav` yet, the
//! program first writes the built-in one there (`apps/base/data/chime.wav`, copy it to the crate's `examples/`
//! directory), so the first run also exercises writing; **it does that only on the card named when building**
//! (`HRX_WRITE_SERIAL=0x...`, the serial number `sd_info` prints). It then reads `/chime.wav` from the card into RAM, parses it,
//! resamples it to the codec's 48.8 kHz and plays it twice; replace the file with any 16-bit PCM WAV of up
//! to 512 KiB (on a PC) to play your own sound. Plug headphones into the **HPH OUT** jack.
//! Needs `hardroxide-audio` and `hardroxide-fat`. Base overlay application 15.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_audio::SAMPLE_RATE_HZ;
use hardroxide_audio::adau1761::Adau1761;
use hardroxide_audio::io::play;
use hardroxide_audio::resample::Resample;
use hardroxide_audio::wav::Wav;
use hardroxide_fat::{OpenMode, Volume};
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;
use ps7::sd::SdHost;

ps7::entry!(main);

static CHIME: &[u8] = include_bytes!("chime.wav");
const MAX_WAV: usize = 512 * 1024;
static mut LOADED: [u8; MAX_WAV] = [0; MAX_WAV];

fn fail(what: &str, e: &dyn core::fmt::Debug) -> ! {
    println!("SD WAV FAILED: {what}: {e:?}");
    halt()
}

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    // SAFETY: the SLCR is read here; the SDIO 0 and I2C 1 windows are used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let base = ps7::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32;
    let mut sd = SdHost::new(be.map(ps7::map::SDIO0_BASE as u64, ps7::map::SDIO_SIZE).unwrap(), base);
    println!("\n== sd_wav ==");
    let info = sd.init().or_else(|_| sd.init_ignoring_card_detect()).unwrap_or_else(|e| fail("card initialisation", &e));
    println!("card: {} MiB", info.blocks / 2048);
    let mut vol = Volume::mount(sd).unwrap_or_else(|e| fail("mount (format the card with sd_erase_and_test first)", &e));

    if vol.stat("/chime.wav").is_err() {
        if !info.serial_matches(option_env!("HRX_WRITE_SERIAL")) {
            println!("REFUSING: the card has no /chime.wav. To let this program write one to the card in the slot, build it with HRX_WRITE_SERIAL={:#010x}", info.serial);
            println!("SD WAV FAILED: not allowed to write");
            halt()
        }
        let mut f = vol.open("/chime.wav", OpenMode::Write).unwrap_or_else(|e| fail("create /chime.wav", &e));
        f.write(&mut vol, CHIME).unwrap_or_else(|e| fail("write /chime.wav", &e));
        f.close(&mut vol).unwrap_or_else(|e| fail("close /chime.wav", &e));
        println!("wrote /chime.wav ({} bytes) from the built-in copy", CHIME.len());
    }

    // SAFETY: LOADED is used only here, once.
    let loaded = unsafe { &mut *core::ptr::addr_of_mut!(LOADED) };
    let mut f = vol.open("/chime.wav", OpenMode::Read).unwrap_or_else(|e| fail("open /chime.wav", &e));
    let size = f.len() as usize;
    if size > MAX_WAV {
        println!("SD WAV FAILED: /chime.wav is {size} bytes, more than {MAX_WAV}");
        halt()
    }
    let mut at = 0;
    while at < size {
        let n = f.read(&mut vol, &mut loaded[at..size]).unwrap_or_else(|e| fail("read /chime.wav", &e));
        if n == 0 {
            println!("SD WAV FAILED: short read at {at} of {size}");
            halt()
        }
        at += n;
    }
    let bytes = &loaded[..size];
    if bytes != CHIME && size == CHIME.len() {
        println!("SD WAV FAILED: /chime.wav read back differs from what was written");
        halt()
    }
    let wav = Wav::parse(bytes).unwrap_or_else(|e| fail("parse WAV", &e));
    println!("/chime.wav: {size} bytes, {} Hz, {} channel(s), {} bits, {} ms", wav.sample_rate, wav.channels, wav.bits, wav.millis());

    let i2c = ps7::i2c::I2c::new(be.map(ps7::map::I2C1_BASE as u64, ps7::map::I2C_SIZE).unwrap());
    let in_hz = ps7::clocks::cpu_1x_hz(&slcr, 50_000_000).unwrap() as u32;
    i2c.set_clock(in_hz, 400_000).unwrap();
    let mut codec = Adau1761::new(i2c);
    if !codec.init().unwrap_or(false) {
        println!("codec bring-up failed");
        halt()
    }
    codec.enable_playback(50).unwrap();
    let mut missed = 0;
    for pass in 1..=2 {
        let frames = Resample::new(wav.frames(), wav.sample_rate, SAMPLE_RATE_HZ as u32);
        let st = play(&d.audio_codec_ctrl_0, frames).unwrap();
        println!("pass {pass}: {} frames played, {} missed", st.frames, st.missed);
        missed += st.missed;
    }
    codec.mute().unwrap();
    println!("{}", if missed == 0 { "SD WAV OK" } else { "SD WAV FAILED: samples were missed" });
    halt()
}
