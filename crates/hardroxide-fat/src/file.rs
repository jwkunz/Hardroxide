//! Files: open, read, write, seek, truncate, close.

use crate::Error;
use crate::bpb::{put16, put32};
use crate::dir::{ATTR_ARCHIVE, Found};
use crate::volume::{Res, Volume};
use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};

/// How to open a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// An existing file, read only.
    Read,
    /// An existing file, reading and writing at the start; contents kept.
    ReadWrite,
    /// Create the file or empty it, then write.
    Write,
    /// Create the file if needed and write at its end.
    Append,
}

/// An open file. It does not borrow the volume: pass the volume to each call, and call
/// [`File::close`] (or [`File::sync`]) after writing so the directory entry is up to date.
#[derive(Debug)]
pub struct File {
    first_cluster: u32,
    size: u32,
    pos: u32,
    /// The directory that holds the file's entry and the entry's index.
    dir: u32,
    short_index: u32,
    /// The cluster that holds byte `cur_ord * cluster_bytes`.
    cur_cluster: u32,
    cur_ord: u32,
    writable: bool,
    dirty: bool,
}

impl File {
    /// Size in bytes.
    pub fn len(&self) -> u32 {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// The current position.
    pub fn pos(&self) -> u32 {
        self.pos
    }

    /// The cluster that holds byte offset `pos`, walking (and, when `extend`, growing) the chain.
    fn cluster_at<D: BlockDevice>(
        &mut self,
        vol: &mut Volume<D>,
        pos: u32,
        extend: bool,
    ) -> Res<Option<u32>, D> {
        let cb = vol.cluster_bytes();
        let ord = pos / cb;
        if self.first_cluster < 2 {
            if !extend {
                return Ok(None);
            }
            let c = vol.alloc_cluster(None)?;
            self.first_cluster = c;
            self.cur_cluster = c;
            self.cur_ord = 0;
            self.dirty = true;
        }
        if self.cur_cluster < 2 || ord < self.cur_ord {
            self.cur_cluster = self.first_cluster;
            self.cur_ord = 0;
        }
        while self.cur_ord < ord {
            match vol.next_cluster(self.cur_cluster)? {
                Some(n) => self.cur_cluster = n,
                None if extend => self.cur_cluster = vol.alloc_cluster(Some(self.cur_cluster))?,
                None => return Ok(None),
            }
            self.cur_ord += 1;
        }
        Ok(Some(self.cur_cluster))
    }

    /// Move the read/write position (within the file's current size or at its end).
    pub fn seek<D: BlockDevice>(&mut self, _vol: &mut Volume<D>, pos: u32) -> Res<(), D> {
        if pos > self.size {
            return Err(Error::Unsupported("seeking past the end of a file"));
        }
        self.pos = pos;
        Ok(())
    }

    /// Read up to `buf.len()` bytes at the current position; returns how many (0 at the end).
    pub fn read<D: BlockDevice>(&mut self, vol: &mut Volume<D>, buf: &mut [u8]) -> Res<usize, D> {
        let want = buf.len().min((self.size - self.pos) as usize);
        let cb = vol.cluster_bytes();
        let mut done = 0;
        while done < want {
            let Some(c) = self.cluster_at(vol, self.pos, false)? else {
                break;
            };
            let off = self.pos % cb;
            let chunk = (want - done).min((cb - off) as usize);
            let lba = vol.cluster_lba(c) + u64::from(off) / BLOCK_SIZE as u64;
            let in_sector = off as usize % BLOCK_SIZE;
            let n = if in_sector == 0 && chunk >= BLOCK_SIZE {
                // Whole sectors straight into the caller's buffer.
                let n = chunk / BLOCK_SIZE * BLOCK_SIZE;
                vol.flush_for(lba, (n / BLOCK_SIZE) as u64)?;
                vol.dev
                    .read_blocks(lba, &mut buf[done..done + n])
                    .map_err(Error::Device)?;
                n
            } else {
                let n = chunk.min(BLOCK_SIZE - in_sector);
                vol.flush_for(lba, 1)?;
                let mut s = [0u8; BLOCK_SIZE];
                vol.dev.read_blocks(lba, &mut s).map_err(Error::Device)?;
                buf[done..done + n].copy_from_slice(&s[in_sector..in_sector + n]);
                n
            };
            done += n;
            self.pos += n as u32;
        }
        Ok(done)
    }

    /// Write `data` at the current position, growing the file; returns how many bytes (all of them,
    /// or fewer when the volume is full).
    pub fn write<D: BlockDevice>(&mut self, vol: &mut Volume<D>, data: &[u8]) -> Res<usize, D> {
        if !self.writable {
            return Err(Error::ReadOnly);
        }
        let cb = vol.cluster_bytes();
        let mut done = 0;
        while done < data.len() {
            let c = match self.cluster_at(vol, self.pos, true) {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(Error::NoSpace) => break,
                Err(e) => return Err(e),
            };
            let off = self.pos % cb;
            let chunk = (data.len() - done).min((cb - off) as usize);
            let lba = vol.cluster_lba(c) + u64::from(off) / BLOCK_SIZE as u64;
            let in_sector = off as usize % BLOCK_SIZE;
            let n = if in_sector == 0 && chunk >= BLOCK_SIZE {
                let n = chunk / BLOCK_SIZE * BLOCK_SIZE;
                vol.invalidate(lba, (n / BLOCK_SIZE) as u64);
                vol.dev
                    .write_blocks(lba, &data[done..done + n])
                    .map_err(Error::Device)?;
                n
            } else {
                let n = chunk.min(BLOCK_SIZE - in_sector);
                let mut s = [0u8; BLOCK_SIZE];
                // A sector wholly past the old end of the file holds nothing worth reading.
                let sector_start = self.pos - in_sector as u32;
                if sector_start < self.size {
                    vol.flush_for(lba, 1)?;
                    vol.dev.read_blocks(lba, &mut s).map_err(Error::Device)?;
                }
                s[in_sector..in_sector + n].copy_from_slice(&data[done..done + n]);
                vol.invalidate(lba, 1);
                vol.dev.write_blocks(lba, &s).map_err(Error::Device)?;
                n
            };
            done += n;
            self.pos += n as u32;
            if self.pos > self.size {
                self.size = self.pos;
                self.dirty = true;
            }
        }
        Ok(done)
    }

    /// Cut the file off at the current position.
    pub fn truncate<D: BlockDevice>(&mut self, vol: &mut Volume<D>) -> Res<(), D> {
        if !self.writable {
            return Err(Error::ReadOnly);
        }
        if self.pos == 0 {
            if self.first_cluster >= 2 {
                vol.free_chain(self.first_cluster)?;
            }
            self.first_cluster = 0;
            self.cur_cluster = 0;
            self.cur_ord = 0;
        } else if let Some(c) = self.cluster_at(vol, self.pos - 1, false)? {
            // Keep the cluster that holds the last byte; free the rest.
            if let Some(n) = vol.next_cluster(c)? {
                vol.cut_chain(c)?;
                vol.free_chain(n)?;
            }
        }
        self.size = self.pos;
        self.dirty = true;
        Ok(())
    }

    /// Write the directory entry (size, first cluster, time) and flush the volume.
    pub fn sync<D: BlockDevice>(&mut self, vol: &mut Volume<D>) -> Res<(), D> {
        if self.dirty {
            let mut e = vol.read_entry(self.dir, self.short_index)?;
            put32(&mut e, 28, self.size);
            put16(&mut e, 20, (self.first_cluster >> 16) as u16);
            put16(&mut e, 26, self.first_cluster as u16);
            e[11] |= ATTR_ARCHIVE;
            put16(&mut e, 22, vol.now.fat_time());
            put16(&mut e, 24, vol.now.fat_date());
            put16(&mut e, 18, vol.now.fat_date());
            vol.write_entry(self.dir, self.short_index, &e)?;
            self.dirty = false;
        }
        vol.sync()
    }

    /// [`File::sync`] and drop the handle.
    pub fn close<D: BlockDevice>(mut self, vol: &mut Volume<D>) -> Res<(), D> {
        self.sync(vol)
    }
}

impl<D: BlockDevice> Volume<D> {
    /// Open a file. `Write` and `Append` create it if it does not exist.
    pub fn open(&mut self, path: &str, mode: OpenMode) -> Res<File, D> {
        let l = self.locate(path)?;
        if l.name.is_empty() {
            return Err(Error::IsADirectory);
        }
        let (parent, name) = (l.parent, l.name);
        let found = l.found;
        let (writable, create) = match mode {
            OpenMode::Read => (false, false),
            OpenMode::ReadWrite => (true, false),
            OpenMode::Write | OpenMode::Append => (true, true),
        };
        let mut f = match found {
            Some(Found {
                entry, short_index, ..
            }) => {
                if entry.is_dir {
                    return Err(Error::IsADirectory);
                }
                File {
                    first_cluster: entry.first_cluster,
                    size: entry.size,
                    pos: 0,
                    dir: parent,
                    short_index,
                    cur_cluster: 0,
                    cur_ord: 0,
                    writable,
                    dirty: false,
                }
            }
            None if create => {
                let short_index = self.create_entry(parent, name, ATTR_ARCHIVE, 0, 0)?;
                File {
                    first_cluster: 0,
                    size: 0,
                    pos: 0,
                    dir: parent,
                    short_index,
                    cur_cluster: 0,
                    cur_ord: 0,
                    writable,
                    dirty: false,
                }
            }
            None => return Err(Error::NotFound),
        };
        match mode {
            OpenMode::Write => f.truncate(self)?,
            OpenMode::Append => f.pos = f.size,
            _ => {}
        }
        Ok(f)
    }

    /// Flush a cached sector to the device before reading it behind the cache's back.
    pub(crate) fn flush_for(&mut self, _lba: u64, _count: u64) -> Res<(), D> {
        // File data never shares a sector with the FAT or with directory data, so nothing cached can
        // be stale; the caches are flushed on `sync`.
        Ok(())
    }

    /// Mark `c` as the last cluster of its chain.
    pub(crate) fn cut_chain(&mut self, c: u32) -> Res<(), D> {
        self.set_eoc(c)
    }
}
