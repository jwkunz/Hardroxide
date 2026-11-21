//! Directories: scanning entries (with long names), looking up paths, creating and removing entries.

use crate::Error;
use crate::bpb::{Timestamp, put16, put32, u16le, u32le};
use crate::name::{
    NameBuf, classify, eq_ignore_case, from_utf16, lfn_checksum, short_to_name, to_utf16,
    valid_name,
};
use crate::volume::{EOC, META, Res, Volume};
use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};

pub(crate) const ATTR_DIR: u8 = 0x10;
pub(crate) const ATTR_VOLUME: u8 = 0x08;
pub(crate) const ATTR_LFN: u8 = 0x0F;
pub(crate) const ATTR_ARCHIVE: u8 = 0x20;
const ENTRY: usize = 32;
const PER_SECTOR: u32 = (BLOCK_SIZE / ENTRY) as u32;
/// Long-name entries hold 13 UTF-16 units each; 255 units need 20.
const MAX_LFN_ENTRIES: usize = 20;
const LFN_OFFSETS: [usize; 13] = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];

/// A directory entry as seen by the caller.
#[derive(Debug, Clone, Copy)]
pub struct DirEntry {
    pub name: NameBuf,
    pub is_dir: bool,
    pub size: u32,
    pub attr: u8,
    pub modified: Timestamp,
    pub(crate) first_cluster: u32,
}

/// An entry with where it lives.
pub(crate) struct Found {
    pub entry: DirEntry,
    pub short: [u8; 11],
    /// The index of the first entry (the first long-name entry, or the short entry itself).
    pub first_index: u32,
    pub short_index: u32,
}

/// Position in a directory's chain of entries.
pub(crate) struct Cursor {
    cluster: u32,
    index: u32,
    end: bool,
}

/// Where a path points.
pub(crate) struct Located<'a> {
    pub parent: u32,
    pub found: Option<Found>,
    pub name: &'a str,
}

impl<D: BlockDevice> Volume<D> {
    fn entries_per_cluster(&self) -> u32 {
        self.spc * PER_SECTOR
    }

    pub(crate) fn cursor(&self, first: u32) -> Cursor {
        Cursor {
            cluster: first,
            index: 0,
            end: false,
        }
    }

    /// The next raw 32-byte entry (every entry, free ones included) until the end of the chain.
    fn next_raw(&mut self, cur: &mut Cursor) -> Res<Option<(u32, [u8; ENTRY])>, D> {
        if cur.end {
            return Ok(None);
        }
        let epc = self.entries_per_cluster();
        let idx = cur.index;
        if idx > 0 && idx.is_multiple_of(epc) {
            match self.next_cluster(cur.cluster)? {
                Some(c) => cur.cluster = c,
                None => {
                    cur.end = true;
                    return Ok(None);
                }
            }
        }
        let lba = self.cluster_lba(cur.cluster) + u64::from((idx % epc) / PER_SECTOR);
        self.load(META, lba)?;
        let off = (idx % PER_SECTOR) as usize * ENTRY;
        let mut e = [0u8; ENTRY];
        e.copy_from_slice(&self.cache_buf(META)[off..off + ENTRY]);
        cur.index += 1;
        Ok(Some((idx, e)))
    }

    /// Visit the named entries of a directory (long names assembled); `f` returns `true` to stop.
    pub(crate) fn scan(&mut self, first: u32, mut f: impl FnMut(&Found) -> bool) -> Res<(), D> {
        let mut cur = self.cursor(first);
        let mut units = [0xFFFFu16; MAX_LFN_ENTRIES * 13];
        let (mut valid, mut expect, mut count, mut sum, mut lfn_first) =
            (false, 0usize, 0usize, 0u8, 0u32);
        while let Some((idx, e)) = self.next_raw(&mut cur)? {
            if e[0] == 0x00 {
                break;
            }
            if e[0] == 0xE5 {
                valid = false;
                continue;
            }
            if e[11] & 0x3F == ATTR_LFN {
                let seq = usize::from(e[0] & 0x3F);
                if e[0] & 0x40 != 0 {
                    valid = (1..=MAX_LFN_ENTRIES).contains(&seq);
                    count = seq;
                    expect = seq;
                    sum = e[13];
                    lfn_first = idx;
                    units.fill(0xFFFF);
                } else if valid && seq + 1 == expect && e[13] == sum {
                    expect = seq;
                } else {
                    valid = false;
                }
                if valid {
                    for (k, &o) in LFN_OFFSETS.iter().enumerate() {
                        units[(seq - 1) * 13 + k] = u16le(&e, o);
                    }
                }
                continue;
            }
            if e[11] & ATTR_VOLUME != 0 {
                valid = false;
                continue;
            }
            let mut short = [0u8; 11];
            short.copy_from_slice(&e[..11]);
            let (name, first_index) = if valid && expect == 1 && lfn_checksum(&short) == sum {
                let all = &units[..count * 13];
                let len = all.iter().position(|&u| u == 0).unwrap_or(all.len());
                (from_utf16(&all[..len]), lfn_first)
            } else {
                (short_to_name(&short, e[12]), idx)
            };
            valid = false;
            let found = Found {
                entry: DirEntry {
                    name,
                    is_dir: e[11] & ATTR_DIR != 0,
                    size: u32le(&e, 28),
                    attr: e[11],
                    modified: Timestamp::from_fat(u16le(&e, 24), u16le(&e, 22)),
                    first_cluster: u32::from(u16le(&e, 20)) << 16 | u32::from(u16le(&e, 26)),
                },
                short,
                first_index,
                short_index: idx,
            };
            if f(&found) {
                break;
            }
        }
        Ok(())
    }

    /// Find `name` (long or short, any case) in the directory starting at `first`.
    pub(crate) fn lookup(&mut self, first: u32, name: &str) -> Res<Option<Found>, D> {
        let mut out = None;
        self.scan(first, |f| {
            let hit = eq_ignore_case(f.entry.name.as_str(), name)
                || eq_ignore_case(short_to_name(&f.short, 0).as_str(), name);
            if hit {
                out = Some(Found {
                    entry: f.entry,
                    short: f.short,
                    first_index: f.first_index,
                    short_index: f.short_index,
                });
            }
            hit
        })?;
        Ok(out)
    }

    /// Resolve `path` up to its last component. Intermediate components must be directories.
    pub(crate) fn locate<'a>(&mut self, path: &'a str) -> Res<Located<'a>, D> {
        let mut dir = self.root;
        let mut comps = path
            .split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .peekable();
        let mut last = "";
        while let Some(c) = comps.next() {
            if comps.peek().is_none() {
                last = c;
                break;
            }
            let f = self.lookup(dir, c)?.ok_or(Error::NotFound)?;
            if !f.entry.is_dir {
                return Err(Error::NotADirectory);
            }
            // A `..` that points at the root stores cluster 0.
            dir = if f.entry.first_cluster == 0 {
                self.root
            } else {
                f.entry.first_cluster
            };
        }
        if last.is_empty() {
            return Ok(Located {
                parent: dir,
                found: None,
                name: "",
            });
        }
        let found = self.lookup(dir, last)?;
        Ok(Located {
            parent: dir,
            found,
            name: last,
        })
    }

    /// The directory a path names (the root for `/`).
    pub(crate) fn dir_cluster(&mut self, path: &str) -> Res<u32, D> {
        let l = self.locate(path)?;
        if l.name.is_empty() {
            return Ok(l.parent);
        }
        let f = l.found.ok_or(Error::NotFound)?;
        if !f.entry.is_dir {
            return Err(Error::NotADirectory);
        }
        Ok(if f.entry.first_cluster == 0 {
            self.root
        } else {
            f.entry.first_cluster
        })
    }

    /// List a directory (`.` and `..` omitted).
    pub fn read_dir(&mut self, path: &str, mut f: impl FnMut(&DirEntry)) -> Res<(), D> {
        let dir = self.dir_cluster(path)?;
        self.scan(dir, |found| {
            let n = found.entry.name.as_str();
            if n != "." && n != ".." {
                f(&found.entry);
            }
            false
        })
    }

    /// Information about a file or directory.
    pub fn stat(&mut self, path: &str) -> Res<DirEntry, D> {
        let l = self.locate(path)?;
        if l.name.is_empty() {
            return Ok(DirEntry {
                name: NameBuf::from_str_lossy("/"),
                is_dir: true,
                size: 0,
                attr: ATTR_DIR,
                modified: self.now,
                first_cluster: self.root,
            });
        }
        l.found.map(|f| f.entry).ok_or(Error::NotFound)
    }

    // ---- writing entries ----

    /// Where entry `index` of the directory starting at `first` lives: `(lba, byte offset)`.
    fn entry_slot(&mut self, first: u32, index: u32) -> Res<(u64, usize), D> {
        let epc = self.entries_per_cluster();
        let mut c = first;
        for _ in 0..index / epc {
            c = self
                .next_cluster(c)?
                .ok_or(Error::Corrupt("directory chain too short"))?;
        }
        let lba = self.cluster_lba(c) + u64::from((index % epc) / PER_SECTOR);
        Ok((lba, (index % PER_SECTOR) as usize * ENTRY))
    }

    pub(crate) fn write_entry(&mut self, first: u32, index: u32, e: &[u8; ENTRY]) -> Res<(), D> {
        let (lba, off) = self.entry_slot(first, index)?;
        self.load(META, lba)?;
        self.cache_buf(META)[off..off + ENTRY].copy_from_slice(e);
        self.mark_dirty(META);
        Ok(())
    }

    pub(crate) fn read_entry(&mut self, first: u32, index: u32) -> Res<[u8; ENTRY], D> {
        let (lba, off) = self.entry_slot(first, index)?;
        self.load(META, lba)?;
        let mut e = [0u8; ENTRY];
        e.copy_from_slice(&self.cache_buf(META)[off..off + ENTRY]);
        Ok(e)
    }

    /// Make sure the directory has room for `entries` entries, adding zeroed clusters.
    fn ensure_capacity(&mut self, first: u32, entries: u32) -> Res<(), D> {
        let epc = self.entries_per_cluster();
        let need = entries.div_ceil(epc);
        let mut have = 1;
        let mut last = first;
        while let Some(n) = self.next_cluster(last)? {
            last = n;
            have += 1;
        }
        while have < need {
            let c = self.alloc_cluster(Some(last))?;
            self.zero_cluster(c)?;
            last = c;
            have += 1;
        }
        Ok(())
    }

    /// Add an entry for `name` to the directory at `first`; returns its short entry's index.
    pub(crate) fn create_entry(
        &mut self,
        first: u32,
        name: &str,
        attr: u8,
        cluster: u32,
        size: u32,
    ) -> Res<u32, D> {
        if !valid_name(name) {
            return Err(Error::InvalidName);
        }
        // An alias no other entry uses.
        let mut n = 1;
        let stored = loop {
            let s = classify(name, n);
            if !s.needs_lfn {
                break s;
            }
            let mut clash = false;
            self.scan(first, |f| {
                clash = f.short == s.short;
                clash
            })?;
            if !clash {
                break s;
            }
            n += 1;
            if n > 999_999 {
                return Err(Error::NoSpace);
            }
        };
        let mut units = [0u16; 255];
        let nunits = to_utf16(name, &mut units);
        let lfn_entries = if stored.needs_lfn {
            nunits.div_ceil(13)
        } else {
            0
        };
        let total = lfn_entries as u32 + 1;

        // A run of `total` free entries (deleted ones, or past the end marker).
        let mut cur = self.cursor(first);
        let (mut run_start, mut run_len) = (0u32, 0u32);
        let mut placed = None;
        while let Some((idx, e)) = self.next_raw(&mut cur)? {
            if e[0] == 0xE5 {
                if run_len == 0 {
                    run_start = idx;
                }
                run_len += 1;
                if run_len >= total {
                    placed = Some(run_start);
                    break;
                }
            } else if e[0] == 0x00 {
                placed = Some(if run_len > 0 { run_start } else { idx });
                break;
            } else {
                run_len = 0;
            }
        }
        let start = match placed {
            Some(s) => s,
            // The chain ended: the entries go where the free run began, or after the last entry.
            None => {
                if run_len > 0 {
                    run_start
                } else {
                    cur.index
                }
            }
        };
        self.ensure_capacity(first, start + total)?;

        let sum = lfn_checksum(&stored.short);
        for k in 0..lfn_entries {
            // The highest ordinal comes first in the directory.
            let ord = lfn_entries - k;
            let mut e = [0u8; ENTRY];
            e[0] = ord as u8 | if k == 0 { 0x40 } else { 0 };
            e[11] = ATTR_LFN;
            e[13] = sum;
            for (j, &o) in LFN_OFFSETS.iter().enumerate() {
                let at = (ord - 1) * 13 + j;
                let u = match at.cmp(&nunits) {
                    core::cmp::Ordering::Less => units[at],
                    core::cmp::Ordering::Equal => 0x0000,
                    core::cmp::Ordering::Greater => 0xFFFF,
                };
                put16(&mut e, o, u);
            }
            self.write_entry(first, start + k as u32, &e)?;
        }
        let mut e = [0u8; ENTRY];
        e[..11].copy_from_slice(&stored.short);
        e[11] = attr;
        e[12] = stored.nt;
        let (date, time) = (self.now.fat_date(), self.now.fat_time());
        put16(&mut e, 14, time);
        put16(&mut e, 16, date);
        put16(&mut e, 18, date);
        put16(&mut e, 20, (cluster >> 16) as u16);
        put16(&mut e, 22, time);
        put16(&mut e, 24, date);
        put16(&mut e, 26, cluster as u16);
        put32(&mut e, 28, size);
        let short_index = start + lfn_entries as u32;
        self.write_entry(first, short_index, &e)?;
        Ok(short_index)
    }

    /// Mark the entries `first_index..=short_index` of a directory deleted.
    pub(crate) fn delete_entries(
        &mut self,
        dir: u32,
        first_index: u32,
        short_index: u32,
    ) -> Res<(), D> {
        for i in first_index..=short_index {
            let mut e = self.read_entry(dir, i)?;
            e[0] = 0xE5;
            self.write_entry(dir, i, &e)?;
        }
        Ok(())
    }

    /// Create a directory.
    pub fn mkdir(&mut self, path: &str) -> Res<(), D> {
        let l = self.locate(path)?;
        if l.name.is_empty() || l.found.is_some() {
            return Err(Error::Exists);
        }
        let (parent, name) = (l.parent, l.name);
        let c = self.alloc_cluster(None)?;
        self.zero_cluster(c)?;
        let (date, time) = (self.now.fat_date(), self.now.fat_time());
        let mut dots = [[0u8; ENTRY]; 2];
        for (i, d) in dots.iter_mut().enumerate() {
            d[..11].copy_from_slice(if i == 0 {
                b".          "
            } else {
                b"..         "
            });
            d[11] = ATTR_DIR;
            put16(d, 14, time);
            put16(d, 16, date);
            put16(d, 18, date);
            put16(d, 22, time);
            put16(d, 24, date);
            let target = if i == 0 {
                c
            } else if parent == self.root {
                0
            } else {
                parent
            };
            put16(d, 20, (target >> 16) as u16);
            put16(d, 26, target as u16);
        }
        self.write_entry(c, 0, &dots[0])?;
        self.write_entry(c, 1, &dots[1])?;
        match self.create_entry(parent, name, ATTR_DIR, c, 0) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.free_chain(c)?;
                Err(e)
            }
        }
    }

    /// Remove a file or an empty directory.
    pub fn remove(&mut self, path: &str) -> Res<(), D> {
        let l = self.locate(path)?;
        let parent = l.parent;
        let f = l.found.ok_or(Error::NotFound)?;
        if f.entry.is_dir {
            let mut empty = true;
            let c = f.entry.first_cluster;
            self.scan(c, |x| {
                let n = x.entry.name.as_str();
                if n != "." && n != ".." {
                    empty = false;
                }
                !empty
            })?;
            if !empty {
                return Err(Error::NotEmpty);
            }
        }
        if f.entry.first_cluster >= 2 {
            self.free_chain(f.entry.first_cluster)?;
        }
        self.delete_entries(parent, f.first_index, f.short_index)
    }
}

#[allow(dead_code)]
const _: u32 = EOC;
