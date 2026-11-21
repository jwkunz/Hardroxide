//! Base overlay, bare metal: **ERASES the SD card in the slot**, formats it as FAT32 and tests it.
//!
//! Safety: it **refuses to run on a card that holds anything**. The first megabyte of the card must be
//! all zeros (a new card) or hold a file system this program formatted before (an "HRXFAT" boot sector at
//! sector 2048). A card with someone else's partition table or data is left alone and the program says why.
//! The owner of a card that does hold data can say it may be erased by naming its serial number (the one
//! the program prints, and Linux shows in `/sys/block/mmcblk0/device/serial`) when building:
//! `HRX_ERASE_SERIAL=0x2df43e3b cargo build --release --example sd_erase_and_test`. Only that one card is accepted.
//!
//! Formats the card (an MBR with one FAT32 partition), writes and reads back directories, long names and
//! files of several sizes with a check on every byte, and reports the speeds of programmed I/O at 25 MHz on a
//! 4-bit bus. Prints `SD TEST OK`. Everything on the card is lost. Needs `hardroxide-fat`. Base overlay application 14.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_fat::{BlockDevice, FormatOptions, OpenMode, Timestamp, Volume};
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;
use ps7::sd::SdHost;

ps7::entry!(main);

fn byte(seed: u32, i: usize) -> u8 {
    (i as u32).wrapping_mul(2_654_435_761).wrapping_add(seed).rotate_left(7) as u8
}

/// The serial number of the one card the owner confirmed may be erased (see the header), if any.
fn confirmed_serial() -> Option<u32> {
    let text = option_env!("HRX_ERASE_SERIAL")?.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    u32::from_str_radix(digits, 16).ok()
}

fn fail(msg: &str) -> ! {
    println!("SD TEST FAILED: {msg}");
    halt()
}

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
    println!("\n== sd_erase_and_test: THIS ERASES THE CARD ==");
    let info = sd.init().or_else(|_| sd.init_ignoring_card_detect()).unwrap_or_else(|e| {
        println!("card initialisation failed: {e}");
        halt()
    });
    println!("card: {} MiB, {}, bus {} Hz, serial {:#010x}", info.blocks / 2048, if info.high_capacity { "SDHC/SDXC" } else { "standard capacity" }, info.clock_hz, info.serial);

    // The safeguard: only an empty card, or one this program formatted before.
    {
        let mut chunk = [0u8; 512 * 64];
        let mut empty = true;
        for part in 0..32u64 {
            sd.read_blocks(part * 64, &mut chunk).unwrap_or_else(|e| fail_e("reading the card", &e));
            if chunk.iter().any(|&b| b != 0) {
                empty = false;
                break;
            }
        }
        let mut sector = [0u8; 512];
        sd.read_blocks(2048, &mut sector).unwrap_or_else(|e| fail_e("reading the card", &e));
        let ours = &sector[3..11] == b"HRXFAT  ";
        let confirmed = confirmed_serial() == Some(info.serial);
        if !empty && !ours && !confirmed {
            println!("REFUSING: this card is not empty and was not formatted by this program, so it is left untouched.");
            println!("Use a new or blank card (all zeros in its first megabyte), or confirm this card by its serial number (see the file header).");
            halt()
        }
        println!("{}", if empty { "the card is empty: proceeding" } else if ours { "the card carries a file system this program formatted: proceeding" } else { "the card's serial number was confirmed by its owner as erasable: proceeding" });
    }

    // Raw blocks at the end of the card, in three sizes (one block, a multi-block run, a long run).
    let mut buf = [0u8; 512 * 64];
    for (blocks, at) in [(1usize, 16u64), (4, 32), (64, 128)] {
        for (i, b) in buf[..blocks * 512].iter_mut().enumerate() {
            *b = byte(blocks as u32, i);
        }
        let lba = info.blocks - at - 8;
        sd.write_blocks(lba, &buf[..blocks * 512]).unwrap_or_else(|e| fail_e("raw write", &e));
        let mut back = [0u8; 512 * 64];
        sd.read_blocks(lba, &mut back[..blocks * 512]).unwrap_or_else(|e| fail_e("raw read", &e));
        if back[..blocks * 512] != buf[..blocks * 512] {
            fail("raw read-back differs");
        }
    }
    println!("raw blocks: 1, 4 and 64 block transfers verified at the end of the card");

    let t0 = ps7::time::now_ns();
    Volume::format(&mut sd, &FormatOptions { label: "HARDROXIDE", ..Default::default() }).unwrap_or_else(|e| fail_e("format", &e));
    println!("formatted in {} ms", (ps7::time::now_ns() - t0) / 1_000_000);
    let mut vol = Volume::mount(sd).unwrap_or_else(|e| fail_e("mount", &e));
    vol.set_time(Timestamp { year: 2026, month: 11, day: 21, hour: 16, minute: 0, second: 0 });
    println!("FAT32: {} MiB free, clusters of {} bytes", vol.free_bytes().unwrap() >> 20, vol.cluster_bytes());

    vol.mkdir("/tests").unwrap();
    vol.mkdir("/tests/nested directory with a long name").unwrap();
    for (i, n) in [0usize, 1, 511, 512, 513, 4095, 4096, 4097, 100_000].into_iter().enumerate() {
        let mut path = [0u8; 40];
        let len = fmt(&mut path, format_args!("/tests/size{i}.bin"));
        let path = core::str::from_utf8(&path[..len]).unwrap();
        let mut f = vol.open(path, OpenMode::Write).unwrap_or_else(|e| fail_e("open", &e));
        let mut at = 0;
        let mut piece = [0u8; 1000];
        while at < n {
            let c = (n - at).min(piece.len());
            for (j, b) in piece[..c].iter_mut().enumerate() {
                *b = byte(i as u32, at + j);
            }
            if f.write(&mut vol, &piece[..c]).unwrap_or_else(|e| fail_e("write", &e)) != c {
                fail("short write");
            }
            at += c;
        }
        f.close(&mut vol).unwrap_or_else(|e| fail_e("close", &e));
        let mut f = vol.open(path, OpenMode::Read).unwrap_or_else(|e| fail_e("reopen", &e));
        if f.len() as usize != n {
            fail("wrong size");
        }
        let mut at = 0;
        while at < n {
            let c = f.read(&mut vol, &mut piece).unwrap_or_else(|e| fail_e("read", &e));
            if c == 0 {
                fail("short read");
            }
            for (j, b) in piece[..c].iter().enumerate() {
                if *b != byte(i as u32, at + j) {
                    fail("file contents differ");
                }
            }
            at += c;
        }
    }
    println!("9 files from 0 to 100000 bytes written and verified byte for byte");

    // Speed: 4 MiB written and read in 32 KiB pieces.
    const PIECE: usize = 32 * 1024;
    let mut big = [0u8; PIECE];
    let t0 = ps7::time::now_ns();
    {
        let mut f = vol.open("/speed.bin", OpenMode::Write).unwrap();
        for n in 0..(4 << 20) / PIECE {
            for (j, b) in big.iter_mut().enumerate() {
                *b = byte(99, n * PIECE + j);
            }
            f.write(&mut vol, &big).unwrap_or_else(|e| fail_e("speed write", &e));
        }
        f.close(&mut vol).unwrap();
    }
    let t1 = ps7::time::now_ns();
    {
        let mut f = vol.open("/speed.bin", OpenMode::Read).unwrap();
        for n in 0..(4 << 20) / PIECE {
            f.read(&mut vol, &mut big).unwrap_or_else(|e| fail_e("speed read", &e));
            if big[0] != byte(99, n * PIECE) || big[PIECE - 1] != byte(99, n * PIECE + PIECE - 1) {
                fail("speed.bin differs");
            }
        }
    }
    let t2 = ps7::time::now_ns();
    println!(
        "4 MiB: write {} ms ({} KiB/s, includes filling the buffer), read {} ms ({} KiB/s)",
        (t1 - t0) / 1_000_000,
        4096 * 1_000_000_000 / (t1 - t0).max(1),
        (t2 - t1) / 1_000_000,
        4096 * 1_000_000_000 / (t2 - t1).max(1)
    );
    vol.sync().unwrap();
    println!("SD TEST OK");
    halt()
}

fn fail_e(what: &str, e: &dyn core::fmt::Debug) -> ! {
    println!("SD TEST FAILED: {what}: {e:?}");
    halt()
}

/// Format into a byte buffer; returns the length.
fn fmt(buf: &mut [u8], args: core::fmt::Arguments) -> usize {
    use core::fmt::Write;
    struct W<'a>(&'a mut [u8], usize);
    impl Write for W<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let n = s.len().min(self.0.len() - self.1);
            self.0[self.1..self.1 + n].copy_from_slice(&s.as_bytes()[..n]);
            self.1 += n;
            Ok(())
        }
    }
    let mut w = W(buf, 0);
    let _ = w.write_fmt(args);
    w.1
}
