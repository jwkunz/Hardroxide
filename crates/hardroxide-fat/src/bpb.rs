//! The boot sector (BIOS parameter block), the partition table and timestamps.

pub(crate) fn u16le(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
pub(crate) fn u32le(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}
pub(crate) fn put16(b: &mut [u8], i: usize, v: u16) {
    b[i..i + 2].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put32(b: &mut [u8], i: usize, v: u32) {
    b[i..i + 4].copy_from_slice(&v.to_le_bytes());
}

/// A calendar time for directory entries (FAT keeps two-second resolution from 1980).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Timestamp {
    /// 2026-01-01 00:00:00, the default until the program sets a time.
    pub const DEFAULT: Timestamp = Timestamp {
        year: 2026,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
    };

    pub(crate) fn fat_date(&self) -> u16 {
        (self.year.clamp(1980, 2107) - 1980) << 9
            | u16::from(self.month.clamp(1, 12)) << 5
            | u16::from(self.day.clamp(1, 31))
    }
    pub(crate) fn fat_time(&self) -> u16 {
        u16::from(self.hour.min(23)) << 11
            | u16::from(self.minute.min(59)) << 5
            | u16::from(self.second.min(59) / 2)
    }
    pub(crate) fn from_fat(date: u16, time: u16) -> Timestamp {
        Timestamp {
            year: 1980 + (date >> 9),
            month: ((date >> 5) & 0xF) as u8,
            day: (date & 0x1F) as u8,
            hour: (time >> 11) as u8,
            minute: ((time >> 5) & 0x3F) as u8,
            second: ((time & 0x1F) * 2) as u8,
        }
    }
}

/// The FAT32 parameters of a volume, read from its boot sector.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bpb {
    pub sectors_per_cluster: u32,
    pub reserved: u32,
    pub num_fats: u32,
    pub fat_sectors: u32,
    pub total_sectors: u32,
    pub root_cluster: u32,
    pub fsinfo: u32,
}

impl Bpb {
    /// Parse a boot sector; `None` if it is not a FAT32 boot sector with 512-byte sectors.
    pub fn parse(s: &[u8]) -> Option<Bpb> {
        if s[510] != 0x55 || s[511] != 0xAA || u16le(s, 11) != 512 {
            return None;
        }
        let spc = u32::from(s[13]);
        let reserved = u32::from(u16le(s, 14));
        let num_fats = u32::from(s[16]);
        let root_entries = u16le(s, 17);
        let tot16 = u32::from(u16le(s, 19));
        let fat16 = u32::from(u16le(s, 22));
        let tot32 = u32le(s, 32);
        let fat32 = u32le(s, 36);
        // FAT32 has no fixed root directory and a 32-bit FAT size.
        if spc == 0
            || !spc.is_power_of_two()
            || root_entries != 0
            || fat16 != 0
            || fat32 == 0
            || num_fats == 0
            || reserved == 0
        {
            return None;
        }
        let total = if tot16 != 0 { tot16 } else { tot32 };
        Some(Bpb {
            sectors_per_cluster: spc,
            reserved,
            num_fats,
            fat_sectors: fat32,
            total_sectors: total,
            root_cluster: u32le(s, 44),
            fsinfo: u32::from(u16le(s, 48)),
        })
    }
}

/// The first FAT32 partition of an MBR, as `(start lba, sectors)`.
pub(crate) fn mbr_partition(s: &[u8]) -> Option<(u64, u64)> {
    if s[510] != 0x55 || s[511] != 0xAA {
        return None;
    }
    for i in 0..4 {
        let e = &s[446 + 16 * i..446 + 16 * (i + 1)];
        if matches!(e[4], 0x0B | 0x0C) {
            return Some((u64::from(u32le(e, 8)), u64::from(u32le(e, 12))));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_round_trip_at_two_second_resolution() {
        let t = Timestamp {
            year: 2026,
            month: 11,
            day: 21,
            hour: 13,
            minute: 7,
            second: 59,
        };
        let back = Timestamp::from_fat(t.fat_date(), t.fat_time());
        assert_eq!(
            (
                back.year,
                back.month,
                back.day,
                back.hour,
                back.minute,
                back.second
            ),
            (2026, 11, 21, 13, 7, 58)
        );
    }

    #[test]
    fn an_empty_sector_is_not_fat32() {
        assert!(Bpb::parse(&[0u8; 512]).is_none());
        assert!(mbr_partition(&[0u8; 512]).is_none());
    }
}
