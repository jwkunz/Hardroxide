//! Base overlay, bare metal: look at the SD card in the slot, without changing it.
//!
//! Identifies the card (capacity, type, manufacturer, speed), mounts its FAT32 file system if it has one
//! and lists the root directory and one level below it with sizes. Needs `hardroxide-fat`. Base overlay application 13.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_fat::Volume;
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;
use ps7::sd::SdHost;

ps7::entry!(main);

fn main() -> ! {
    let _d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    // SAFETY: the SLCR is read here; the SDIO 0 window is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let base = ps7::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32;
    let mut sd = SdHost::new(be.map(ps7::map::SDIO0_BASE as u64, ps7::map::SDIO_SIZE).unwrap(), base);
    println!("\n== sd_info (SD base clock {base} Hz) ==");
    println!("card detect pin: {}", if sd.card_present().unwrap() { "card present" } else { "no card seen (trying anyway)" });
    let info = match sd.init().or_else(|_| sd.init_ignoring_card_detect()) {
        Ok(i) => i,
        Err(e) => {
            println!("card initialisation failed: {e}");
            halt()
        }
    };
    println!(
        "card: {} blocks = {} MiB, {}, manufacturer {:#04x}, product {:?}, serial {:#010x}, bus {} Hz",
        info.blocks,
        info.blocks / 2048,
        if info.high_capacity { "SDHC/SDXC" } else { "standard capacity" },
        info.manufacturer,
        core::str::from_utf8(&info.product).unwrap_or("?"),
        info.serial,
        info.clock_hz
    );
    let mut vol = match Volume::mount(sd) {
        Ok(v) => v,
        Err(e) => {
            println!("no FAT32 file system on the card ({e}); `sd_erase_and_test` formats it");
            halt()
        }
    };
    println!("FAT32: {} MiB free of {} MiB, clusters of {} bytes", vol.free_bytes().unwrap() >> 20, vol.total_bytes() >> 20, vol.cluster_bytes());
    let mut dirs = [hardroxide_fat::NameBuf::new(); 8];
    let mut n = 0;
    vol.read_dir("/", |e| {
        println!("  {} {:>10}  {}", if e.is_dir { "d" } else { "-" }, e.size, e.name);
        if e.is_dir && n < dirs.len() {
            dirs[n] = e.name;
            n += 1;
        }
    })
    .unwrap();
    for d in &dirs[..n] {
        println!("  {}/", d);
        let mut path = [0u8; 64];
        let p = {
            let name = d.as_str().as_bytes();
            path[0] = b'/';
            let len = name.len().min(62);
            path[1..1 + len].copy_from_slice(&name[..len]);
            core::str::from_utf8(&path[..1 + len]).unwrap()
        };
        vol.read_dir(p, |e| println!("      {} {:>10}  {}", if e.is_dir { "d" } else { "-" }, e.size, e.name)).unwrap();
    }
    halt()
}
