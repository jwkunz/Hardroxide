# hardroxide-dma

Physically contiguous buffers a DMA engine can be pointed at.

```rust,ignore
pub trait DmaBuffer
fn len(&self) -> usize;
fn phys_addr(&self) -> u64;
fn as_slice(&self) -> &[u8];
fn as_mut_slice(&mut self) -> &mut [u8];
```

`phys_addr` is the bus address to program into the engine. Ownership by value
is how hardware access is tracked: [`AxiDma::start`](ip-dma.md) moves the
buffer into a `Transfer` and only returns it once the channel is known to be
stopped.

## `Udmabuf` (default backend)

```rust,ignore
pub struct Udmabuf
pub fn open(index: u32) -> Result<Self, DmaError>
pub fn open_with(paths: &UdmabufPaths) -> Result<Self, DmaError>
pub struct UdmabufPaths
pub dev: PathBuf,
pub phys_addr: PathBuf,
pub size: PathBuf,
pub fn for_index(n: u32) -> Self
```

Backed by the `u-dma-buf` kernel module: `/dev/udmabufN` is mapped with
`O_SYNC` (uncached), the physical address is read from
`/sys/class/u-dma-buf/udmabufN/phys_addr` (hex with `0x` or decimal) and the
size from `.../size`. Uncached means the CPU is slow on the buffer but no
cache maintenance is ever needed (ADR 0001). **Unverified on hardware**
([G10](../08-verification-gaps.md)).

Any `DmaBuffer` also works as the memory behind an HLS pointer argument
([`set_<arg>_buffer`](ip-hls.md#pointer-arguments-m_axi-offsetslave)).

## `MockBuffer`

```rust,ignore
pub struct MockBuffer
pub fn new(len: usize, phys: u64) -> Self
```

Heap memory with a *fake* physical address, for tests with the mock MMIO
backend. Hardware cannot DMA to it.

`DmaError`: `Mmio`, `Io { path, source }`, `Parse(text, path)`.
