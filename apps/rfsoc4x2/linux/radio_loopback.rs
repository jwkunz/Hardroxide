//! RFSoC 4x2 base overlay under Linux: send a tone from each DAC, record it with an ADC and find it in the spectrum.
//!
//! SMA cables from DAC A to ADC A and from DAC B to ADC B. Run as root, with `u-dma-buf` loaded (`/dev/udmabuf0` and
//! `1`, at least 64 KiB each). It prints one line per check, then `RADIO LOOPBACK OK`. `common/loopback.rs` goes in
//! `common/` next to this file.
//!
//! **The converters, and the radio blocks that run on their clocks, need the board's clock chips programmed.** Until
//! then an access to those blocks never completes and the board has to be switched off. The PYNQ image programs the
//! chips while starting (`/boot/boot.py`); the program refuses to run unless that has happened. If you program them
//! some other way, say so with `radio_loopback --clocks-are-programmed`.
use design_1::Design;
use hardroxide_dma::{DmaBuffer, Udmabuf};
use std::error::Error;
use std::process::Command;

#[path = "common/loopback.rs"]
mod loopback;
use loopback::{BUFFER_BYTES, Bench, Receiver, SAMPLES, Spectrum, Transmitter};

/// Whether the image's start-up script has run and programmed the clocks: `systemctl show`'s answer about the service
/// that runs it, and the script.
fn started_the_clocks(service: &str, script: &str) -> Result<(), &'static str> {
    let property = |name: &str| {
        service
            .lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('='))
    };
    if !script.lines().any(|l| {
        let l = l.trim_start();
        !l.starts_with('#') && (l.contains("init_rf_clks(") || l.contains("set_ref_clks("))
    }) {
        return Err("/boot/boot.py does not program the clocks");
    }
    if matches!(property("ExecMainExitTimestampMonotonic"), None | Some("0")) {
        return Err(
            "the start-up script has not finished yet: wait about two minutes after power-on",
        );
    }
    if property("Result") != Some("success") || property("ExecMainStatus") != Some("0") {
        return Err("the start-up script failed (journalctl -u bootpy)");
    }
    Ok(())
}

fn check_clocks() -> Result<(), Box<dyn Error>> {
    let service = Command::new("systemctl")
        .args([
            "show",
            "bootpy.service",
            "--property=Result,ExecMainStatus,ExecMainExitTimestampMonotonic",
        ])
        .output()?;
    let script = std::fs::read_to_string("/boot/boot.py").unwrap_or_default();
    started_the_clocks(&String::from_utf8_lossy(&service.stdout), &script)
        .map_err(|why| format!("not touching the radio: {why}").into())
}

fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        None => check_clocks()?,
        Some("--clocks-are-programmed") => {}
        Some(other) => return Err(format!("unknown argument `{other}`").into()),
    }
    let mut d = Design::load()?;
    println!("== RFSoC 4x2 base overlay: radio loopback ==");
    let buffers = (Udmabuf::open(0)?, Udmabuf::open(1)?);
    if buffers.0.len() < BUFFER_BYTES || buffers.1.len() < BUFFER_BYTES {
        return Err(format!("/dev/udmabuf0 and 1 need {BUFFER_BYTES} bytes each").into());
    }
    let mut re = vec![0.0; SAMPLES].into_boxed_slice();
    let mut im = vec![0.0; SAMPLES].into_boxed_slice();

    macro_rules! receiver {
        ($name:literal, $tile:literal, $block:literal, $generator:ident, $i:ident, $q:ident) => {
            Receiver {
                name: $name,
                tile: $tile,
                block: $block,
                generator: &mut d.$generator,
                i: &d.$i,
                q: &d.$q,
            }
        };
    }
    let mut print = |text: std::fmt::Arguments| print!("{text}");
    let failures = loopback::measure(Bench {
        rfdc: &mut d.radio_rfdc,
        transmitters: [
            Transmitter {
                name: "DAC A",
                tile: 2,
                amplitude: &mut d.radio_transmitter_channel_20_amplitude_controller,
            },
            Transmitter {
                name: "DAC B",
                tile: 0,
                amplitude: &mut d.radio_transmitter_channel_00_amplitude_controller,
            },
        ],
        receivers: [
            receiver!(
                "ADC A (receiver 21)",
                2,
                1,
                radio_receiver_channel_21_packet_generator,
                radio_receiver_channel_21_axi_dma_real,
                radio_receiver_channel_21_axi_dma_imag
            ),
            receiver!(
                "ADC B (receiver 20)",
                2,
                0,
                radio_receiver_channel_20_packet_generator,
                radio_receiver_channel_20_axi_dma_real,
                radio_receiver_channel_20_axi_dma_imag
            ),
            receiver!(
                "ADC C (receiver 01)",
                0,
                1,
                radio_receiver_channel_01_packet_generator,
                radio_receiver_channel_01_axi_dma_real,
                radio_receiver_channel_01_axi_dma_imag
            ),
            receiver!(
                "ADC D (receiver 00)",
                0,
                0,
                radio_receiver_channel_00_packet_generator,
                radio_receiver_channel_00_axi_dma_real,
                radio_receiver_channel_00_axi_dma_imag
            ),
        ],
        buffers: Some(buffers),
        spectrum: Spectrum::new(
            (&mut *re).try_into().expect("the length"),
            (&mut *im).try_into().expect("the length"),
        ),
        print: &mut print,
        failures: 0,
    });
    if failures != 0 {
        return Err(format!("RADIO LOOPBACK FAILED: {failures} check(s)").into());
    }
    println!("RADIO LOOPBACK OK");
    Ok(())
}
