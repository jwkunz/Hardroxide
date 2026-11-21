//! FAT32 over a [`BlockDevice`].
//!
//! * [`Volume`]: mount a device (a partitioned card or a bare FAT32 volume), list directories, open,
//!   read, write, create, remove, make directories, sync.
//! * [`Volume::format`]: write a partition table and an empty FAT32 file system.
//! * Long file names are read and written (UTF-16 entries with the short alias, as other systems
//!   expect); names are handled as ASCII-case-insensitive UTF-8 of at most 255 bytes.
//!
//! `no_std` and allocation-free: the volume keeps two 512-byte sector caches (one for the FAT, one for
//! directory data); file data goes straight between the caller's buffer and the device in whole
//! sectors and clusters.
//!
//! Not supported: FAT12 and FAT16, exFAT, multiple partitions (the first FAT32 partition is used),
//! and file times other than the one set with [`Volume::set_time`].

#![cfg_attr(not(test), no_std)]

mod bpb;
mod dir;
mod file;
mod format;
mod name;
mod volume;

pub use bpb::Timestamp;
pub use dir::DirEntry;
pub use file::{File, OpenMode};
pub use format::FormatOptions;
pub use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};
pub use name::NameBuf;
pub use volume::Volume;

/// Errors from the file system. `E` is the device's error type.
#[derive(Debug, thiserror::Error)]
pub enum Error<E: core::fmt::Debug> {
    #[error("device error: {0:?}")]
    Device(E),
    #[error("not a FAT32 file system")]
    NotFat32,
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
    #[error("no such file or directory")]
    NotFound,
    #[error("already exists")]
    Exists,
    #[error("not a directory")]
    NotADirectory,
    #[error("is a directory")]
    IsADirectory,
    #[error("directory not empty")]
    NotEmpty,
    #[error("no space left on the device")]
    NoSpace,
    #[error("invalid name")]
    InvalidName,
    #[error("the file system is damaged: {0}")]
    Corrupt(&'static str),
    #[error("the file was not opened for writing")]
    ReadOnly,
}
