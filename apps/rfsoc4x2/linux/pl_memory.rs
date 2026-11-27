//! RFSoC 4x2 base overlay under Linux: the FPGA's own 4 GiB of DDR4, beside the memory Linux runs in.
//!
//! The processor reaches it through the generated crate's `ddr4_0` window. The program checks the address lines, writes
//! and reads back patterns at the start, the middle and the end, and reports the speed of the processor's accesses.
//! It overwrites what it tests. Run as root.
//!
//! **The memory's reference clock comes from the board's clock chip.** Until that chip has been programmed since
//! power-on, an access to the memory never completes and the board has to be switched off. The PYNQ image programs it
//! while starting (`/boot/boot.py`); the program refuses to run unless that has happened. If you program the chip some
//! other way, say so with `pl_memory --clocks-are-programmed`.
use design_1::Design;
use hardroxide_mmio::Mmio;
use std::error::Error;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The memory's size: the design's window is twice this, and the memory appears in it twice.
const SIZE: usize = 4 << 30;
/// How much is tested at each of the three places.
const BLOCK: usize = 4 << 20;

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
        .map_err(|why| format!("not touching the memory: {why}").into())
}

/// A value for the word at `offset` that differs between neighbours and between far places.
fn word(offset: usize, seed: u32) -> u32 {
    ((offset / 4) as u32).wrapping_mul(0x9E37_79B9) ^ seed
}

fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        None => check_clocks()?,
        Some("--clocks-are-programmed") => {}
        Some(other) => return Err(format!("unknown argument `{other}`").into()),
    }
    // Loads the bitstream; the memory's controller then calibrates, which takes a moment.
    let d = Design::load()?;
    sleep(Duration::from_millis(500));
    let memory = &d.ddr4_0;
    println!("== the FPGA's DDR4 ==");

    // One word at the start and one at every power of two: a stuck or shorted address line makes two of them collide.
    let places: Vec<usize> = std::iter::once(0)
        .chain((2..32).map(|bit| 1usize << bit))
        .collect();
    for (n, &at) in places.iter().enumerate() {
        memory.write32(at, 0xA5A5_0000 | n as u32)?;
    }
    for (n, &at) in places.iter().enumerate() {
        let got = memory.read32(at)?;
        if got != 0xA5A5_0000 | n as u32 {
            return Err(format!("address {at:#x} reads {got:#010x}").into());
        }
    }
    println!("{} address lines: ok", places.len() - 1);
    memory.write32(SIZE, 0x0DD0_C0DE)?;
    let repeats = memory.read32(0)? == 0x0DD0_C0DE;
    println!("4 GiB, appearing again at +4 GiB: {repeats}");

    let (mut writing, mut reading) = (Duration::ZERO, Duration::ZERO);
    for (name, start) in [("start", 0), ("middle", SIZE / 2), ("end", SIZE - BLOCK)] {
        for seed in [0u32, u32::MAX] {
            let t = Instant::now();
            for at in (start..start + BLOCK).step_by(4) {
                memory.write32(at, word(at, seed))?;
            }
            writing += t.elapsed();
            let t = Instant::now();
            for at in (start..start + BLOCK).step_by(4) {
                let got = memory.read32(at)?;
                if got != word(at, seed) {
                    return Err(format!("{name}: {at:#x} reads {got:#010x}").into());
                }
            }
            reading += t.elapsed();
        }
        println!("{} MiB at the {name}, two patterns: ok", BLOCK >> 20);
    }
    // Two patterns at three places.
    let bytes = (6 * BLOCK) as f64;
    println!(
        "from the processor, a word at a time: writes {:.1} MB/s, reads {:.1} MB/s",
        bytes / writing.as_secs_f64() / 1e6,
        bytes / reading.as_secs_f64() / 1e6
    );
    println!("PL MEMORY OK");
    Ok(())
}
