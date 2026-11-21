//! A mounted FAT32 volume: the sector caches, the allocation table, clusters.

use crate::Error;
use crate::bpb::{Bpb, Timestamp, mbr_partition, put32, u32le};
use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};

pub(crate) const MASK: u32 = 0x0FFF_FFFF;
pub(crate) const EOC: u32 = 0x0FFF_FFFF;
const EOC_MIN: u32 = 0x0FFF_FFF8;
const BAD: u32 = 0x0FFF_FFF7;
/// The fewest clusters a FAT32 volume has (fewer means FAT16 by the specification's rule).
const MIN_CLUSTERS: u32 = 65_525;

pub(crate) type Res<T, D> = Result<T, Error<<D as BlockDevice>::Error>>;

pub(crate) struct Cache {
    lba: u64,
    dirty: bool,
    pub(crate) buf: [u8; BLOCK_SIZE],
}

impl Cache {
    const fn new() -> Self {
        Cache { lba: u64::MAX, dirty: false, buf: [0; BLOCK_SIZE] }
    }
}

pub(crate) const FAT: usize = 0;
pub(crate) const META: usize = 1;

/// A mounted FAT32 file system.
pub struct Volume<D: BlockDevice> {
    pub(crate) dev: D,
    pub(crate) spc: u32,
    pub(crate) fat_lba: u64,
    pub(crate) fat_sectors: u32,
    pub(crate) num_fats: u32,
    pub(crate) data_lba: u64,
    pub(crate) clusters: u32,
    pub(crate) root: u32,
    fsinfo_lba: u64,
    free_count: Option<u32>,
    next_free: u32,
    fsinfo_dirty: bool,
    caches: [Cache; 2],
    pub(crate) now: Timestamp,
}

impl<D: BlockDevice> Volume<D> {
    /// Mount the first FAT32 file system on `dev`: a bare volume, or the first FAT32 partition of an MBR.
    pub fn mount(mut dev: D) -> Res<Self, D> {
        let mut s = [0u8; BLOCK_SIZE];
        dev.read_blocks(0, &mut s).map_err(Error::Device)?;
        let (part, bpb) = match Bpb::parse(&s) {
            Some(b) => (0u64, b),
            None => {
                let (start, _) = mbr_partition(&s).ok_or(Error::NotFat32)?;
                dev.read_blocks(start, &mut s).map_err(Error::Device)?;
                (start, Bpb::parse(&s).ok_or(Error::NotFat32)?)
            }
        };
        let fat_lba = part + u64::from(bpb.reserved);
        let data_lba = fat_lba + u64::from(bpb.num_fats) * u64::from(bpb.fat_sectors);
        let data_sectors = bpb.total_sectors.saturating_sub(bpb.reserved + bpb.num_fats * bpb.fat_sectors);
        let clusters = data_sectors / bpb.sectors_per_cluster;
        if clusters < MIN_CLUSTERS {
            return Err(Error::NotFat32);
        }
        if bpb.root_cluster < 2 || bpb.root_cluster >= clusters + 2 {
            return Err(Error::Corrupt("root cluster out of range"));
        }
        let mut v = Volume {
            dev,
            spc: bpb.sectors_per_cluster,
            fat_lba,
            fat_sectors: bpb.fat_sectors,
            num_fats: bpb.num_fats,
            data_lba,
            clusters,
            root: bpb.root_cluster,
            fsinfo_lba: if bpb.fsinfo != 0 && bpb.fsinfo != 0xFFFF { part + u64::from(bpb.fsinfo) } else { 0 },
            free_count: None,
            next_free: 2,
            fsinfo_dirty: false,
            caches: [Cache::new(), Cache::new()],
            now: Timestamp::DEFAULT,
        };
        if v.fsinfo_lba != 0 {
            let mut f = [0u8; BLOCK_SIZE];
            v.dev.read_blocks(v.fsinfo_lba, &mut f).map_err(Error::Device)?;
            if u32le(&f, 0) == 0x4161_5252 && u32le(&f, 484) == 0x6141_7272 {
                let free = u32le(&f, 488);
                let next = u32le(&f, 492);
                if free <= clusters {
                    v.free_count = Some(free);
                }
                if (2..clusters + 2).contains(&next) {
                    v.next_free = next;
                }
            }
        }
        Ok(v)
    }

    /// The time stamped on files and directories created or changed from now on.
    pub fn set_time(&mut self, t: Timestamp) {
        self.now = t;
    }

    /// Bytes per cluster.
    pub fn cluster_bytes(&self) -> u32 {
        self.spc * BLOCK_SIZE as u32
    }

    /// Bytes of data area (all clusters).
    pub fn total_bytes(&self) -> u64 {
        u64::from(self.clusters) * u64::from(self.cluster_bytes())
    }

    /// Free bytes. Uses the count the volume keeps; counts the allocation table once if it has none.
    pub fn free_bytes(&mut self) -> Res<u64, D> {
        if self.free_count.is_none() {
            let mut free = 0;
            for c in 2..self.clusters + 2 {
                if self.fat_get(c)? == 0 {
                    free += 1;
                }
            }
            self.free_count = Some(free);
            self.fsinfo_dirty = true;
        }
        Ok(u64::from(self.free_count.unwrap_or(0)) * u64::from(self.cluster_bytes()))
    }

    /// Write everything cached back and flush the device.
    pub fn sync(&mut self) -> Res<(), D> {
        self.flush_cache(FAT)?;
        self.flush_cache(META)?;
        if self.fsinfo_dirty && self.fsinfo_lba != 0 {
            let mut f = [0u8; BLOCK_SIZE];
            self.dev.read_blocks(self.fsinfo_lba, &mut f).map_err(Error::Device)?;
            if u32le(&f, 0) == 0x4161_5252 {
                put32(&mut f, 488, self.free_count.unwrap_or(0xFFFF_FFFF));
                put32(&mut f, 492, self.next_free);
                self.dev.write_blocks(self.fsinfo_lba, &f).map_err(Error::Device)?;
            }
            self.fsinfo_dirty = false;
        }
        self.dev.flush().map_err(Error::Device)
    }

    /// Sync and give the device back.
    pub fn into_device(mut self) -> Res<D, D> {
        self.sync()?;
        Ok(self.dev)
    }

    // ---- the sector caches ----

    pub(crate) fn flush_cache(&mut self, which: usize) -> Res<(), D> {
        let (lba, dirty) = (self.caches[which].lba, self.caches[which].dirty);
        if dirty {
            if which == FAT {
                // Every copy of the table gets the same sector.
                for k in 0..self.num_fats {
                    let at = lba + u64::from(k) * u64::from(self.fat_sectors);
                    self.dev.write_blocks(at, &self.caches[which].buf).map_err(Error::Device)?;
                }
            } else {
                self.dev.write_blocks(lba, &self.caches[which].buf).map_err(Error::Device)?;
            }
            self.caches[which].dirty = false;
        }
        Ok(())
    }

    pub(crate) fn load(&mut self, which: usize, lba: u64) -> Res<(), D> {
        if self.caches[which].lba == lba {
            return Ok(());
        }
        self.flush_cache(which)?;
        self.caches[which].lba = u64::MAX;
        let mut tmp = [0u8; BLOCK_SIZE];
        self.dev.read_blocks(lba, &mut tmp).map_err(Error::Device)?;
        self.caches[which].buf = tmp;
        self.caches[which].lba = lba;
        Ok(())
    }

    pub(crate) fn cache_buf(&mut self, which: usize) -> &mut [u8; BLOCK_SIZE] {
        &mut self.caches[which].buf
    }

    pub(crate) fn mark_dirty(&mut self, which: usize) {
        self.caches[which].dirty = true;
    }

    /// Forget the cached sector if it is `lba` (after the sector was written behind the cache's back).
    pub(crate) fn invalidate(&mut self, lba: u64, count: u64) {
        for c in &mut self.caches {
            if c.lba >= lba && c.lba < lba + count {
                c.lba = u64::MAX;
                c.dirty = false;
            }
        }
    }

    // ---- the allocation table ----

    pub(crate) fn cluster_lba(&self, c: u32) -> u64 {
        self.data_lba + u64::from(c - 2) * u64::from(self.spc)
    }

    pub(crate) fn fat_get(&mut self, c: u32) -> Res<u32, D> {
        let lba = self.fat_lba + u64::from(c) * 4 / BLOCK_SIZE as u64;
        self.load(FAT, lba)?;
        Ok(u32le(&self.caches[FAT].buf, (c as usize * 4) % BLOCK_SIZE) & MASK)
    }

    fn fat_set(&mut self, c: u32, v: u32) -> Res<(), D> {
        let lba = self.fat_lba + u64::from(c) * 4 / BLOCK_SIZE as u64;
        self.load(FAT, lba)?;
        let off = (c as usize * 4) % BLOCK_SIZE;
        let old = u32le(&self.caches[FAT].buf, off);
        put32(&mut self.caches[FAT].buf, off, (old & !MASK) | (v & MASK));
        self.caches[FAT].dirty = true;
        Ok(())
    }

    /// The cluster after `c` in its chain, or `None` at the end.
    pub(crate) fn next_cluster(&mut self, c: u32) -> Res<Option<u32>, D> {
        let v = self.fat_get(c)?;
        if v >= EOC_MIN {
            Ok(None)
        } else if v < 2 || v == BAD || v >= self.clusters + 2 {
            Err(Error::Corrupt("bad cluster chain"))
        } else {
            Ok(Some(v))
        }
    }

    /// Allocate a cluster, link it after `prev` if given, and return it. The cluster's contents are
    /// not cleared.
    pub(crate) fn alloc_cluster(&mut self, prev: Option<u32>) -> Res<u32, D> {
        let span = self.clusters;
        let mut c = self.next_free.clamp(2, self.clusters + 1);
        for _ in 0..span {
            if self.fat_get(c)? == 0 {
                self.fat_set(c, EOC)?;
                if let Some(p) = prev {
                    self.fat_set(p, c)?;
                }
                self.next_free = if c + 1 >= self.clusters + 2 { 2 } else { c + 1 };
                if let Some(n) = &mut self.free_count {
                    *n = n.saturating_sub(1);
                }
                self.fsinfo_dirty = true;
                return Ok(c);
            }
            c = if c + 1 >= self.clusters + 2 { 2 } else { c + 1 };
        }
        Err(Error::NoSpace)
    }

    pub(crate) fn set_eoc(&mut self, c: u32) -> Res<(), D> {
        self.fat_set(c, EOC)
    }

    /// Free a whole chain starting at `c`.
    pub(crate) fn free_chain(&mut self, mut c: u32) -> Res<(), D> {
        if c < 2 {
            return Ok(());
        }
        loop {
            let next = self.next_cluster(c)?;
            self.fat_set(c, 0)?;
            if let Some(n) = &mut self.free_count {
                *n += 1;
            }
            self.fsinfo_dirty = true;
            if self.next_free > c {
                self.next_free = c;
            }
            match next {
                Some(n) => c = n,
                None => return Ok(()),
            }
        }
    }

    /// Fill a cluster with zeros.
    pub(crate) fn zero_cluster(&mut self, c: u32) -> Res<(), D> {
        let lba = self.cluster_lba(c);
        self.invalidate(lba, u64::from(self.spc));
        let zeros = [0u8; BLOCK_SIZE];
        for i in 0..u64::from(self.spc) {
            self.dev.write_blocks(lba + i, &zeros).map_err(Error::Device)?;
        }
        Ok(())
    }
}
