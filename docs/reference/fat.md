# hardroxide-fat

A FAT32 file system over a [block device](mmio.md#block-devices) (an SD card, a disk image in memory): mount, directories with long names, read and write files,
create and delete, and format a whole card. `no_std`, no allocation, no dependencies beyond the Hardroxide block trait. Written for this
project and tested against `dosfstools` (`mkfs.fat` builds the images it reads, `fsck.fat -n` checks the images it writes), under QEMU's
SD card model, and on a PYNQ-Z2 with a real card (see [Verification gaps](../08-verification-gaps.md)).

```rust,ignore
pub struct Volume<D: BlockDevice>
pub fn mount(mut dev: D) -> Res<Self, D>
pub fn format(dev: &mut D, opts: &FormatOptions) -> Res<(), D>
pub fn open(&mut self, path: &str, mode: OpenMode) -> Res<File, D>
pub fn stat(&mut self, path: &str) -> Res<DirEntry, D>
pub fn read_dir(&mut self, path: &str, mut f: impl FnMut(&DirEntry)) -> Res<(), D>
pub fn mkdir(&mut self, path: &str) -> Res<(), D>
pub fn remove(&mut self, path: &str) -> Res<(), D>
pub fn set_time(&mut self, t: Timestamp)
pub fn cluster_bytes(&self) -> u32
pub fn total_bytes(&self) -> u64
pub fn free_bytes(&mut self) -> Res<u64, D>
pub fn sync(&mut self) -> Res<(), D>
pub fn into_device(mut self) -> Res<D, D>
pub struct File
pub fn len(&self) -> u32
pub fn is_empty(&self) -> bool
pub fn pos(&self) -> u32
pub fn seek<D: BlockDevice>(&mut self, _vol: &mut Volume<D>, pos: u32) -> Res<(), D>
pub fn read<D: BlockDevice>(&mut self, vol: &mut Volume<D>, buf: &mut [u8]) -> Res<usize, D>
pub fn write<D: BlockDevice>(&mut self, vol: &mut Volume<D>, data: &[u8]) -> Res<usize, D>
pub fn truncate<D: BlockDevice>(&mut self, vol: &mut Volume<D>) -> Res<(), D>
pub fn sync<D: BlockDevice>(&mut self, vol: &mut Volume<D>) -> Res<(), D>
pub fn close<D: BlockDevice>(mut self, vol: &mut Volume<D>) -> Res<(), D>
pub enum OpenMode
pub struct DirEntry
pub struct FormatOptions<'a>
pub struct Timestamp
pub struct NameBuf
pub const fn new() -> Self
pub fn as_str(&self) -> &str
pub enum Error<E: core::fmt::Debug>
```

`Res<T, D>` is `Result<T, Error<D::Error>>`. A `File` does not borrow the volume: pass the volume to each call, and call `close` (or `sync`) after
writing, so the directory entry carries the final size.

- **Mounting** reads sector 0. A card is usually partitioned: if sector 0 is an MBR, the first FAT32 partition (type `0x0B` or `0x0C`) is used; otherwise
  the device is taken to be one bare volume (a "superfloppy"). Anything that is not FAT32 with 512-byte sectors is `Error::NotFat32` or `Error::Unsupported`.
- **Paths** use `/` and are case-insensitive for lookup. Names are UTF-8 of up to 255 bytes. Long names are read and written (with an 8.3 alias such as
  `LONGNA~1.TXT` and the NT lower-case flags), `NameBuf` holds one on the stack.
- **`OpenMode`:** `Read` (an existing file), `ReadWrite` (existing, contents kept), `Write` (create or empty) and `Append` (create, write at the end).
- **`read_dir`** calls the closure for each entry (not `.` and `..`) with its `name`, `is_dir`, `size`, `attr` and `modified`. `remove` deletes a file or an
  empty directory (`Error::NotEmpty` otherwise). There is no rename.
- **Format:** `Volume::format` writes an MBR with one FAT32 partition starting at 1 MiB (what cameras and computers expect of a card), or with
  `partition_table: false` a bare volume. Clusters are 512 bytes to 32 KiB depending on the size (4 KiB up to 8 GiB, 16 KiB up to 32 GiB). It writes the boot sector and its backup,
  the `FSInfo` sector, both FAT copies and an empty root directory, and clears the reserved area, the FATs and the root cluster (the data area is not
  touched, so old contents remain readable by a recovery tool). A 30 GiB card took 15 s over the PS SD controller in programmed I/O.
- **Caches:** one FAT sector and one directory sector are cached (written through when another is needed and by `sync`); file data goes straight to the device. Call `sync` (or `into_device`) before the card is removed.
- **Time:** directory entries are stamped with the time given to `set_time` (FAT keeps two-second resolution from 1980); the default is 2026-01-01. The PS has
  no real-time clock to read it from.
- **Not supported:** FAT12/16, exFAT, 4 KiB sectors, a second partition, rename, links, file times other than the modified time, and files over 4 GiB (a FAT limit).
- **Errors:** `Device` wraps the block device's error; `NotFat32`, `Unsupported`, `NotFound`, `Exists`, `NotADirectory`, `IsADirectory`, `NotEmpty`, `NoSpace`,
  `InvalidName`, `Corrupt` (an inconsistent file system, for example a bad cluster chain or a root cluster out of range) and `ReadOnly` (a write to a file opened for reading).
