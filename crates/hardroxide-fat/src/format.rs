//! Writing a new file system: a partition table and an empty FAT32 volume.

use crate::Error;
use crate::bpb::{put16, put32};
use crate::volume::{Res, Volume};
use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};

/// Options for [`Volume::format`].
#[derive(Debug, Clone, Copy)]
pub struct FormatOptions<'a> {
    /// The volume label (up to 11 characters; upper case, as FAT stores it).
    pub label: &'a str,
    /// Write an MBR with one FAT32 partition starting at 1 MiB (what cameras and computers expect of
    /// a card); `false` makes the whole device one bare volume.
    pub partition_table: bool,
    /// The volume serial number.
    pub volume_id: u32,
}

impl Default for FormatOptions<'_> {
    fn default() -> Self {
        FormatOptions {
            label: "HARDROXIDE",
            partition_table: true,
            volume_id: 0x4852_5831,
        }
    }
}

const RESERVED: u32 = 32;
const PARTITION_START: u64 = 2048;

fn zero<D: BlockDevice>(dev: &mut D, lba: u64, count: u64) -> Res<(), D> {
    let zeros = [0u8; 8 * BLOCK_SIZE];
    let mut done = 0;
    while done < count {
        let n = (count - done).min(8);
        dev.write_blocks(lba + done, &zeros[..n as usize * BLOCK_SIZE])
            .map_err(Error::Device)?;
        done += n;
    }
    Ok(())
}

/// Sectors per cluster by volume size (Microsoft's table for FAT32), lowered until the volume has
/// enough clusters to be FAT32 at all.
fn pick_spc(total: u64) -> u32 {
    match total {
        0..=532_480 => 1,
        532_481..=16_777_216 => 8,
        16_777_217..=33_554_432 => 16,
        33_554_433..=67_108_864 => 32,
        _ => 64,
    }
}

impl<D: BlockDevice> Volume<D> {
    /// Write an empty FAT32 file system on `dev`, destroying what is there.
    pub fn format(dev: &mut D, opts: &FormatOptions) -> Res<(), D> {
        let total = dev.block_count();
        let start = if opts.partition_table {
            PARTITION_START
        } else {
            0
        };
        if total <= start + 70_000 {
            return Err(Error::Unsupported("device too small for FAT32"));
        }
        let part = total - start;
        if part > u64::from(u32::MAX) {
            return Err(Error::Unsupported(
                "device too large for a 32-bit sector count",
            ));
        }
        let part32 = part as u32;
        let mut spc = pick_spc(part);
        let (fat_sectors, clusters) = loop {
            let mut fat = 1u32;
            let clusters = loop {
                let data = part32 - RESERVED - 2 * fat;
                let clusters = data / spc;
                let need = (clusters + 2).div_ceil(BLOCK_SIZE as u32 / 4);
                if need <= fat {
                    break clusters;
                }
                fat = need;
            };
            if clusters >= 65_525 || spc == 1 {
                break (fat, clusters);
            }
            spc /= 2;
        };
        if clusters < 65_525 {
            return Err(Error::Unsupported("device too small for FAT32"));
        }

        // The partition table.
        if opts.partition_table {
            let mut mbr = [0u8; BLOCK_SIZE];
            let p = &mut mbr[446..462];
            p[0] = 0x00;
            p[1..4].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
            p[4] = 0x0C; // FAT32 with LBA addressing
            p[5..8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
            put32(&mut mbr, 446 + 8, start as u32);
            put32(&mut mbr, 446 + 12, part32);
            mbr[510] = 0x55;
            mbr[511] = 0xAA;
            dev.write_blocks(0, &mbr).map_err(Error::Device)?;
        }

        // The reserved area: boot sector, FSInfo, their backups, and zeros between.
        zero(dev, start, u64::from(RESERVED))?;
        let mut boot = [0u8; BLOCK_SIZE];
        boot[..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
        boot[3..11].copy_from_slice(b"HRXFAT  ");
        put16(&mut boot, 11, BLOCK_SIZE as u16);
        boot[13] = spc as u8;
        put16(&mut boot, 14, RESERVED as u16);
        boot[16] = 2;
        boot[21] = 0xF8;
        put16(&mut boot, 24, 63);
        put16(&mut boot, 26, 255);
        put32(&mut boot, 28, start as u32);
        put32(&mut boot, 32, part32);
        put32(&mut boot, 36, fat_sectors);
        put32(&mut boot, 44, 2);
        put16(&mut boot, 48, 1);
        put16(&mut boot, 50, 6);
        boot[64] = 0x80;
        boot[66] = 0x29;
        put32(&mut boot, 67, opts.volume_id);
        let mut label = [b' '; 11];
        for (d, s) in label.iter_mut().zip(opts.label.bytes()) {
            *d = s.to_ascii_uppercase();
        }
        boot[71..82].copy_from_slice(&label);
        boot[82..90].copy_from_slice(b"FAT32   ");
        boot[510] = 0x55;
        boot[511] = 0xAA;
        let mut info = [0u8; BLOCK_SIZE];
        put32(&mut info, 0, 0x4161_5252);
        put32(&mut info, 484, 0x6141_7272);
        put32(&mut info, 488, clusters - 1); // the root directory uses one
        put32(&mut info, 492, 3);
        put32(&mut info, 508, 0xAA55_0000);
        for (at, sector) in [(0u64, &boot), (1, &info), (6, &boot), (7, &info)] {
            dev.write_blocks(start + at, sector)
                .map_err(Error::Device)?;
        }

        // The two allocation tables: zeros, then the first sector of each.
        let fat0 = start + u64::from(RESERVED);
        zero(dev, fat0, 2 * u64::from(fat_sectors))?;
        let mut first = [0u8; BLOCK_SIZE];
        put32(&mut first, 0, 0x0FFF_FFF8);
        put32(&mut first, 4, 0x0FFF_FFFF);
        put32(&mut first, 8, 0x0FFF_FFFF); // cluster 2, the root directory, is one cluster long
        dev.write_blocks(fat0, &first).map_err(Error::Device)?;
        dev.write_blocks(fat0 + u64::from(fat_sectors), &first)
            .map_err(Error::Device)?;

        // The root directory: one cluster holding the volume label.
        let root = fat0 + 2 * u64::from(fat_sectors);
        zero(dev, root, u64::from(spc))?;
        let mut dirsec = [0u8; BLOCK_SIZE];
        dirsec[..11].copy_from_slice(&label);
        dirsec[11] = 0x08;
        dev.write_blocks(root, &dirsec).map_err(Error::Device)?;
        dev.flush().map_err(Error::Device)
    }
}
