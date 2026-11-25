//! End-to-end test of the SD host driver and the FAT32 file system: initialise the card, format it, write files and
//! directories, read them back and check them, and report the speeds. Runs under QEMU, where the "card" is a disk image
//! (`scripts/test-qemu.sh` then runs `fsck.fat` on it), and on a board over JTAG with a card in the slot, **which it
//! erases**. For a board it must be built for that one card: `HRX_ERASE_SERIAL=0x...` (the serial number `sd-info`
//! prints); any other card is left untouched. Afterwards it leaves the start of the card (to 16 MiB past the file system's
//! tables) in memory for the host to read back and check.

#![no_std]
#![no_main]

use hardroxide_fat::{FormatOptions, OpenMode, Timestamp, Volume};
use platform::ps::{self, cpu, mem, println, time};

#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;
#[path = "../../common/sd.rs"]
mod sd;

ps::entry!(main);

/// The card a build for a board may erase.
const ERASE_SERIAL: Option<&str> = option_env!("HRX_ERASE_SERIAL");
/// Where the start of the card is left in memory.
const DUMP_AT: usize = 0x1000_0000;

fn may_erase(serial: u32) -> bool {
    cfg!(feature = "qemu")
        || ERASE_SERIAL.and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            == Some(serial)
}

/// A deterministic byte pattern.
fn byte(seed: u32, i: usize) -> u8 {
    (i as u32)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(seed)
        .rotate_left(7) as u8
}

fn halt() -> ! {
    loop {
        cpu::wfe()
    }
}

fn fail(msg: &str) -> ! {
    println!("SD SMOKE FAILED: {msg}");
    halt()
}

fn main() -> ! {
    platform::init("SD card and FAT32 smoke test");
    let (mut sd, _) = sd::host();
    let info = match sd.init() {
        Ok(i) => i,
        Err(e) => fail(&alloc_free(format_args!("card initialisation: {e}"))),
    };
    println!(
        "card: {} blocks ({} MiB), {}, rca {:#x}, bus {} Hz, serial {:#010x}",
        info.blocks,
        info.blocks * 512 / (1 << 20),
        if info.high_capacity {
            "SDHC/SDXC"
        } else {
            "standard capacity"
        },
        info.rca,
        info.clock_hz,
        info.serial
    );
    // Nothing has been written so far, and nothing is unless this is the card the build names.
    if !may_erase(info.serial) {
        println!(
            "SD SMOKE REFUSED: this build may erase {} and this card is {:#010x}; nothing was written",
            ERASE_SERIAL.unwrap_or("no card"),
            info.serial
        );
        halt();
    }

    // Raw blocks: write a pattern at the end of the card, read it back.
    let mut block = [0u8; 512 * 4];
    for (i, b) in block.iter_mut().enumerate() {
        *b = byte(1, i);
    }
    let last = info.blocks - 8;
    sd.write(last, &block)
        .unwrap_or_else(|e| fail(&alloc_free(format_args!("raw write: {e}"))));
    let mut back = [0u8; 512 * 4];
    sd.read(last, &mut back)
        .unwrap_or_else(|e| fail(&alloc_free(format_args!("raw read: {e}"))));
    if back != block {
        fail("raw block read-back differs");
    }
    println!("raw blocks: 4 blocks written and read back at the end of the card");

    // The file system.
    Volume::format(&mut sd, &FormatOptions::default())
        .unwrap_or_else(|e| fail(&alloc_free(format_args!("format: {e}"))));
    let mut vol =
        Volume::mount(sd).unwrap_or_else(|e| fail(&alloc_free(format_args!("mount: {e}"))));
    vol.set_time(Timestamp {
        year: 2026,
        month: 11,
        day: 21,
        hour: 15,
        minute: 0,
        second: 0,
    });
    println!(
        "formatted and mounted: {} MiB free, {} byte clusters",
        vol.free_bytes().unwrap() >> 20,
        vol.cluster_bytes()
    );

    vol.mkdir("/logs").unwrap();
    {
        let mut f = vol.open("/logs/report.txt", OpenMode::Write).unwrap();
        f.write(&mut vol, b"hardroxide SD smoke test\n").unwrap();
        f.close(&mut vol).unwrap();
    }
    {
        let mut f = vol
            .open("/A long file name for the smoke test.txt", OpenMode::Write)
            .unwrap();
        f.write(&mut vol, b"long names work\n").unwrap();
        f.close(&mut vol).unwrap();
    }

    // A megabyte, written in 4 KiB pieces and read back.
    const MIB: usize = 1 << 20;
    let mut chunk = [0u8; 4096];
    let t0 = time::now_ns();
    {
        let mut f = vol.open("/big.bin", OpenMode::Write).unwrap();
        for n in 0..MIB / 4096 {
            for (i, b) in chunk.iter_mut().enumerate() {
                *b = byte(7, n * 4096 + i);
            }
            if f.write(&mut vol, &chunk).unwrap() != 4096 {
                fail("short write");
            }
        }
        f.close(&mut vol).unwrap();
    }
    let t1 = time::now_ns();
    {
        let mut f = vol.open("/big.bin", OpenMode::Read).unwrap();
        if f.len() as usize != MIB {
            fail("big.bin has the wrong size");
        }
        for n in 0..MIB / 4096 {
            if f.read(&mut vol, &mut chunk).unwrap() != 4096 {
                fail("short read");
            }
            for (i, b) in chunk.iter().enumerate() {
                if *b != byte(7, n * 4096 + i) {
                    fail("big.bin read-back differs");
                }
            }
        }
    }
    let t2 = time::now_ns();
    println!(
        "1 MiB written in {} ms, read in {} ms (4 KiB pieces, programmed I/O)",
        (t1 - t0) / 1_000_000,
        (t2 - t1) / 1_000_000
    );

    let mut names = 0;
    vol.read_dir("/", |e| {
        println!(
            "  {} {:>8} {}",
            if e.is_dir { "d" } else { "-" },
            e.size,
            e.name
        );
        names += 1;
    })
    .unwrap();
    if names != 3 {
        fail("the root directory should hold 3 entries");
    }
    vol.sync().unwrap();
    // The start of the card in memory, for the host: the partition table, the file system's tables and the first 16 MiB of
    // its data, read through a driver of its own (so the card is identified afresh).
    if !cfg!(feature = "qemu") {
        let (mut sd, _) = sd::host();
        sd.init()
            .unwrap_or_else(|e| fail(&alloc_free(format_args!("second initialisation: {e}"))));
        let mut boot = [0u8; 512];
        sd.read(2048, &mut boot).unwrap();
        let reserved = u64::from(u16::from_le_bytes([boot[14], boot[15]]));
        let per_fat = u64::from(u32::from_le_bytes([boot[36], boot[37], boot[38], boot[39]]));
        let blocks = 2048 + reserved + u64::from(boot[16]) * per_fat + 32768;
        let len = blocks as usize * 512;
        // SAFETY: DDR beyond the program, used by nothing else; the DDR is at least 512 MiB.
        let dump = unsafe { core::slice::from_raw_parts_mut(DUMP_AT as *mut u8, len) };
        let t = time::now_ns();
        sd.read(0, dump)
            .unwrap_or_else(|e| fail(&alloc_free(format_args!("dump: {e}"))));
        let ms = (time::now_ns() - t) / 1_000_000;
        mem::flush_range(DUMP_AT, len);
        println!("read {} MiB in one run in {ms} ms", len >> 20);
        println!("DUMP {DUMP_AT:#x} {len}");
    }
    println!("SD SMOKE OK");
    halt()
}

/// Format a message into a small stack buffer (no allocator here).
fn alloc_free(args: core::fmt::Arguments) -> heapless_str::Str {
    heapless_str::Str::new(args)
}

mod heapless_str {
    use core::fmt::Write;
    pub struct Str {
        buf: [u8; 160],
        len: usize,
    }
    impl Str {
        pub fn new(args: core::fmt::Arguments) -> Self {
            let mut s = Str {
                buf: [0; 160],
                len: 0,
            };
            let _ = s.write_fmt(args);
            s
        }
    }
    impl Write for Str {
        fn write_str(&mut self, t: &str) -> core::fmt::Result {
            let n = t.len().min(self.buf.len() - self.len);
            self.buf[self.len..self.len + n].copy_from_slice(&t.as_bytes()[..n]);
            self.len += n;
            Ok(())
        }
    }
    impl core::ops::Deref for Str {
        type Target = str;
        fn deref(&self) -> &str {
            core::str::from_utf8(&self.buf[..self.len]).unwrap_or("?")
        }
    }
}
