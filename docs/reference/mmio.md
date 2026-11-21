# hardroxide-mmio

Memory-mapped register access behind a pluggable backend, so drivers run
unchanged on `/dev/mem`, UIO, or a mock.

## Features and `no_std`

| Feature | Default | Adds |
|---|---|---|
| `std` | yes | the `/dev/mem`, UIO and mock backends, `MmioError::Io`, `std::time` as the default clock |
| `physmem` | no (always present without `std`) | the [`PhysMem`](#physical-memory-backend-bare-metal) backend |

Without `std` the crate is `#![no_std]` (it builds for `armv7a-none-eabihf`; see
`scripts/check-no-std.sh`). `MmioError` gains `BadAddress(u64)` (a physical address that
does not fit in a pointer on this target); `Io` exists only with `std`.

## Traits

```rust,ignore
pub trait Mmio
fn len(&self) -> usize;
fn read32(&self, offset: usize) -> Result<u32, MmioError>;
fn write32(&self, offset: usize, value: u32) -> Result<(), MmioError>;
fn modify32(&self, offset: usize, f: impl FnOnce(u32) -> u32) -> Result<(), MmioError>
pub trait MmioBackend
type Region: Mmio;
fn map(&self, phys: u64, len: usize) -> Result<Self::Region, MmioError>;
```

- Offsets are **bytes** from the start of the window and must be 4-byte
  aligned; accesses are 32-bit.
- Methods take `&self`: a register write is not a Rust-level mutation of memory
  the program owns. Callers are responsible for hardware-level races.
- `modify32` is a plain read-modify-write (not atomic); beware registers whose
  reads have side effects.
- `MmioBackend::map` maps `len` bytes of *physical* address space starting at
  `phys`.

`MmioError`: `OutOfBounds { offset, len }` (offset or offset+4 beyond the
window), `Unaligned(offset)`, `Io { what, source }`.

## Time

Drivers measure timeouts with this crate's `Instant`, so the same driver code runs on
Linux and without an operating system.

```rust,ignore
pub type TimeSource = fn() -> u64;
pub fn set_time_source(source: TimeSource)
pub struct Instant
pub fn now() -> Self
pub fn elapsed(&self) -> Duration
pub fn duration_since(&self, earlier: Instant) -> Duration
pub fn checked_add(&self, d: Duration) -> Option<Instant>
pub fn sleep(d: Duration)
```

With `std`, `Instant::now()` defaults to `std::time::Instant`. Without it there is nothing
to ask, so the platform must call `set_time_source` once at start-up with a function that
returns monotonic nanoseconds; until then `Instant::now()` panics rather than let a
timeout loop spin forever. `set_time_source` is one atomic store, so it can be called from
any context. `elapsed` saturates at zero if the clock steps backwards. `sleep` sleeps the
thread with `std` and busy-waits on the time source without it.

## `/dev/mem` backend

```rust,ignore
pub struct DevMem
pub fn new() -> Self
pub fn with_path(path: impl Into<PathBuf>) -> Self
pub struct SysRegion
pub fn map_file(file: &std::fs::File, file_offset: u64, len: usize, what: &str) -> Result<Self, MmioError>
pub fn as_ptr(&self) -> *mut u8
```

`DevMem` opens `/dev/mem` (or the path given to `with_path`, useful for
testing against a regular file) with `O_RDWR|O_SYNC` and `mmap`s the window;
unaligned `phys` values are handled. Needs root; may be blocked by
`CONFIG_STRICT_DEVMEM` on some kernels. `SysRegion` is the mapping, unmapped
on drop; `map_file` maps any open file/device (used by the DMA buffers) and
`as_ptr` exposes the start of the window, valid for `len()` bytes while the
region lives. This crate contains all of the workspace's `unsafe` hardware
access; each use is documented in source.

## Physical-memory backend (bare metal)

```rust,ignore
pub struct PhysMem
pub const unsafe fn new() -> Self
pub struct PhysRegion
pub fn address(&self) -> usize
```

For programs with no operating system: physical addresses are used as pointers, with
volatile reads and writes and no `mmap`. `PhysMem::new` is `unsafe` because every region
it hands out is a raw window onto hardware: the caller asserts that addresses are
identity-mapped (with device-appropriate memory attributes for registers) and that nothing
else uses the windows it maps. Writes are preceded by a data synchronisation barrier on
ARM, so an earlier write to a buffer is visible before the register write that tells a
device to use it. `PhysRegion` is `Send` (ownership of a window can move to another task)
but not `Sync`. Unit tests use a heap buffer standing in for registers.

## UIO backend

```rust,ignore
pub struct Uio
pub fn new(path: impl Into<PathBuf>) -> Self
```

Maps map 0 of a `/dev/uioN` device. The requested physical address is ignored
(the kernel owns the window). Interrupt waiting is in
[hardroxide-irq](irq.md). Verified on hardware (Timer through `/dev/uioN`).

## Mock backend

```rust,ignore
pub struct MockBackend
pub fn poke(&self, phys: u64, value: u32)
pub fn peek(&self, phys: u64) -> u32
pub fn set_write1_clear(&self, phys: u64)
pub fn set_write1_clear_mask(&self, phys: u64, mask: u32)
pub fn log(&self) -> Vec<Access>
```

(`MockBackend::new()` creates one; it is `Clone`, and clones share memory.)
Sparse fake physical memory: registers read back what was written (0 if never
written). `poke`/`peek` access memory directly without logging; region reads
and writes are recorded in `log()` as `MockAccess::Read/Write { addr, value }`.
`set_write1_clear_mask` makes the masked bits of a register write-1-to-clear
(a write clears the bits set in the value; other bits store normally), for
modelling status registers.

## Block devices

```rust,ignore
pub const BLOCK_SIZE: usize = 512;
pub type Block = [u8; BLOCK_SIZE];
pub trait BlockDevice
pub enum SliceDiskError
pub struct SliceDisk<'a>
pub fn new(bytes: &'a mut [u8]) -> Self
```

`hardroxide_mmio::block`: storage addressed in 512-byte blocks, the meeting point of a card driver and a file system, so neither depends on the other.
`BlockDevice` has an associated `Error`, `block_count()`, `read_blocks(lba, buf)`, `write_blocks(lba, buf)` (the buffer is a whole number of blocks, may be
many) and `flush()` (default: nothing). `SliceDisk` is a disk in memory over a byte slice, used by the tests of [`hardroxide-fat`](fat.md) and for images
loaded into RAM. The PS SD controller driver ([`ps7::sd`](ps7.md#sd-card-controller)) implements the trait.
