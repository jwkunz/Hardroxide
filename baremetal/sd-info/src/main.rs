//! Look at the SD card in the slot **without being able to change it**: identify it, list its partitions, say what file
//! systems they hold and list the root of a FAT32 one. Every access to the card goes through [`ReadOnly`], which has no way
//! to write. Runs on the boards over JTAG (the target chooses the processor family) and under QEMU.

#![no_std]
#![no_main]

use hardroxide_fat::Volume;
use hardroxide_mmio::PhysRegion;
use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};
use platform::ps::{self, cpu, println};
use ps::sd::{SdError, SdHost};

#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;
#[path = "../../common/sd.rs"]
mod sd;

ps::entry!(main);

/// The card, with writing taken away.
struct ReadOnly<'a>(&'a mut SdHost<PhysRegion>);

#[derive(Debug)]
#[allow(dead_code)]
enum ReadError {
    Card(SdError),
    WriteRefused,
}

impl BlockDevice for ReadOnly<'_> {
    type Error = ReadError;
    fn block_count(&self) -> u64 {
        self.0.block_count()
    }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        self.0.read(lba, buf).map_err(ReadError::Card)
    }
    fn write_blocks(&mut self, _: u64, _: &[u8]) -> Result<(), ReadError> {
        Err(ReadError::WriteRefused)
    }
}

fn text(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("?")
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// What file system starts at block `lba`, from its signatures.
fn look(card: &mut ReadOnly, lba: u64) {
    let mut b = [0u8; 4 * BLOCK_SIZE];
    if let Err(e) = card.read_blocks(lba, &mut b) {
        println!("    cannot read: {e:?}");
        return;
    }
    let boot = &b[..BLOCK_SIZE];
    let ext = &b[1024..];
    if ext[0x38] == 0x53 && ext[0x39] == 0xEF {
        println!(
            "    ext2/3/4 file system, label {:?}, last mounted on {:?}",
            text(&ext[0x78..0x88]),
            text(&ext[0x88..0xC8])
        );
        // Superblock times are seconds since 1970; the block counts tell how full it is.
        println!(
            "    created {}, last mounted {}, last written {} (Unix time), mounted {} times, {} of {} blocks free",
            le32(ext, 0x108),
            le32(ext, 0x2C),
            le32(ext, 0x30),
            u16::from_le_bytes([ext[0x34], ext[0x35]]),
            le32(ext, 0x0C),
            le32(ext, 0x04)
        );
    } else if &boot[82..87] == b"FAT32" {
        println!("    FAT32, label {:?}", text(&boot[71..82]));
    } else if &boot[54..57] == b"FAT" {
        println!(
            "    {}, label {:?}",
            text(&boot[54..62]),
            text(&boot[43..54])
        );
        let reserved = u64::from(u16::from_le_bytes([boot[14], boot[15]]));
        let per_fat = u64::from(u16::from_le_bytes([boot[22], boot[23]]));
        let root = lba + reserved + u64::from(boot[16]) * per_fat;
        let entries = usize::from(u16::from_le_bytes([boot[17], boot[18]]));
        short_names(card, root, entries.div_ceil(16).min(32) as u64);
    } else if &boot[3..11] == b"EXFAT   " {
        println!("    exFAT");
    } else if &boot[3..11] == b"NTFS    " {
        println!("    NTFS");
    } else if b.iter().all(|&v| v == 0) {
        println!("    the first 2 KiB are zero");
    } else {
        println!("    no file system recognised");
    }
}

/// The short names in a FAT12/16 root directory of `blocks` blocks at `lba`.
fn short_names(card: &mut ReadOnly, lba: u64, blocks: u64) {
    let mut b = [0u8; BLOCK_SIZE];
    for n in 0..blocks {
        if card.read_blocks(lba + n, &mut b).is_err() {
            return;
        }
        for e in b.chunks(32) {
            match (e[0], e[11]) {
                (0, _) => return,
                (0xE5, _) | (_, 0x0F) | (_, 0x08) => {} // deleted, long-name part, label
                (_, attributes) => println!(
                    "      {}.{}  {} bytes{}",
                    text(&e[0..8]).trim_end(),
                    text(&e[8..11]).trim_end(),
                    le32(e, 28),
                    if attributes & 0x10 != 0 {
                        " (directory)"
                    } else {
                        ""
                    }
                ),
            }
        }
    }
}

fn main() -> ! {
    platform::init("sd-info (read only)");
    let (mut sd, base) = sd::host();
    println!(
        "controller base clock {base} Hz, card detect: {:?}",
        sd.card_present()
    );
    let info = match sd.init() {
        Ok(i) => i,
        Err(e) => {
            println!("card initialisation failed: {e}\nSD INFO FAILED");
            loop {
                cpu::wfe();
            }
        }
    };
    println!(
        "card: manufacturer {:#04x}, product {:?}, SERIAL {:#010x}",
        info.manufacturer,
        text(&info.product),
        info.serial
    );
    println!(
        "      {} blocks = {} MB = {} MiB, {}, bus {} Hz",
        info.blocks,
        info.blocks * 512 / 1_000_000,
        info.blocks / 2048,
        if info.high_capacity {
            "SDHC/SDXC"
        } else {
            "standard capacity"
        },
        info.clock_hz
    );

    let mut card = ReadOnly(&mut sd);
    // Is anything at the start of the card?
    let mut chunk = [0u8; 64 * BLOCK_SIZE];
    let mut used = 0u32;
    for n in 0..32u64 {
        match card.read_blocks(n * 64, &mut chunk) {
            Ok(()) => {
                used += chunk
                    .chunks(BLOCK_SIZE)
                    .filter(|b| b.iter().any(|&v| v != 0))
                    .count() as u32
            }
            Err(e) => println!("read error in the first MiB: {e:?}"),
        }
    }
    println!("first MiB: {used} of 2048 blocks hold something other than zero");

    let mut mbr = [0u8; BLOCK_SIZE];
    card.read_blocks(0, &mut mbr).unwrap();
    if mbr[510] == 0x55 && mbr[511] == 0xAA {
        let mut any = false;
        for i in 0..4 {
            let e = &mbr[446 + 16 * i..462 + 16 * i];
            let (kind, start, count) = (e[4], le32(e, 8), le32(e, 12));
            if kind != 0 && count != 0 && u64::from(start) < card.block_count() {
                any = true;
                println!(
                    "partition {}: type {kind:#04x}, starts at block {start}, {} MiB",
                    i + 1,
                    count / 2048
                );
                if kind == 0xEE {
                    println!("    (a GPT-partitioned card)");
                } else {
                    look(&mut card, u64::from(start));
                }
            }
        }
        if !any {
            println!("no partitions in the table; block 0:");
            look(&mut card, 0);
        }
    } else {
        println!("no partition table; block 0:");
        look(&mut card, 0);
    }

    match Volume::mount(card) {
        Ok(mut vol) => {
            println!(
                "root of the FAT32 volume ({} MiB, {} MiB free):",
                vol.total_bytes() >> 20,
                vol.free_bytes().unwrap_or(0) >> 20
            );
            let mut n = 0u32;
            let listed = vol.read_dir("/", |e| {
                n += 1;
                if n <= 40 {
                    println!(
                        "  {} {:>11}  {}",
                        if e.is_dir { "d" } else { "-" },
                        e.size,
                        e.name
                    );
                }
            });
            println!(
                "  {n} entries{}",
                if listed.is_err() {
                    " (listing failed)"
                } else {
                    ""
                }
            );
        }
        Err(e) => println!("no FAT32 volume mounted: {e}"),
    }
    println!("SD INFO DONE (nothing was written)");
    loop {
        cpu::wfe();
    }
}
