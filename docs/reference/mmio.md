# hardroxide-mmio

Memory-mapped register access behind a pluggable backend, so drivers run
unchanged on `/dev/mem`, UIO, or a mock.

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

## UIO backend

```rust,ignore
pub struct Uio
pub fn new(path: impl Into<PathBuf>) -> Self
```

Maps map 0 of a `/dev/uioN` device. The requested physical address is ignored
(the kernel owns the window). Interrupt waiting is in
[hardroxide-irq](irq.md). Unverified on hardware.

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
