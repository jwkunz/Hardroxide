# AXI BRAM Controller

`hardroxide_ip::bram` — `xilinx.com:ip:axi_bram_ctrl` (PG078). The mapped
window *is* the block RAM, so the driver is a bounds-checked, word-addressed
view over it. All offsets are byte offsets and must be 4-byte aligned.

```rust,ignore
pub struct AxiBram<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn len(&self) -> usize
pub fn is_empty(&self) -> bool
pub fn read_word(&self, offset: usize) -> Result<u32, MmioError>
pub fn write_word(&self, offset: usize, value: u32) -> Result<(), MmioError>
pub fn read_words(&self, offset: usize, out: &mut [u32]) -> Result<(), MmioError>
pub fn write_words(&self, offset: usize, words: &[u32]) -> Result<(), MmioError>
pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), MmioError>
pub fn write_bytes(&self, offset: usize, data: &[u8]) -> Result<(), MmioError>
```

- `len` is the size of the mapped window (the HWH address range).
- Bytes are packed little-endian into words. `read_bytes` reads a final
  partial word whole and truncates; `write_bytes` read-modify-writes a final
  partial word so neighbouring bytes keep their value.
- Out-of-range or unaligned offsets return `MmioError::OutOfBounds` /
  `Unaligned`; a multi-word access that runs off the end fails part-way (words
  before the end have already been written).
- The generated `bram_test` example fills the whole memory with a pattern and
  reads it back.
- Dual-port or ECC configurations are not specially handled; the second port is
  not visible to software.
