# AXI4-Stream FIFO

`hardroxide_ip::fifo` — `xilinx.com:ip:axi_fifo_mm_s` (PG080) in AXI4-Lite
register mode: packets are moved by programmed I/O, 32 bits at a time.
Packet bytes are packed little-endian; the length in bytes is exact (the last
word is zero-padded on send and truncated on receive).

```rust,ignore
pub const ISR: usize = 0x00;
pub const IER: usize = 0x04;
pub const TDFR: usize = 0x08;
pub const TDFV: usize = 0x0C;
pub const TDFD: usize = 0x10;
pub const TLR: usize = 0x14;
pub const RDFR: usize = 0x18;
pub const RDFO: usize = 0x1C;
pub const RDFD: usize = 0x20;
pub const RLR: usize = 0x24;
pub const SRR: usize = 0x28;
pub enum FifoError
pub struct AxiFifo<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn reset(&self) -> Result<(), FifoError>
pub fn tx_vacancy(&self) -> Result<usize, FifoError>
pub fn rx_occupancy(&self) -> Result<usize, FifoError>
pub fn interrupt_status(&self) -> Result<u32, FifoError>
pub fn clear_interrupts(&self, mask: u32) -> Result<(), FifoError>
pub fn set_interrupt_mask(&self, mask: u32) -> Result<(), FifoError>
pub fn send(&self, data: &[u8]) -> Result<(), FifoError>
pub fn try_recv(&self) -> Result<Option<Vec<u8>>, FifoError>
pub fn recv(&self, timeout: Duration) -> Result<Option<Vec<u8>>, FifoError>
pub fn recv_into(&self, buf: &mut [u8], timeout: Duration) -> Result<Option<usize>, FifoError>
```

- `reset` writes the `0xA5` key to `SRR`, `TDFR` and `RDFR` and clears every
  interrupt flag.
- `send` checks `TDFV` first: if the packet does not fit whole it returns
  `FifoError::NoRoom` and writes nothing. Otherwise it writes the words to
  `TDFD` and then the byte length to `TLR`, which starts the packet. Empty
  packets are rejected (`EmptyPacket`).
- `try_recv` returns the next complete packet or `None` (no wait). A packet is
  available when `RDFO` is non-zero; the driver reads `RLR` (the byte count,
  low 23 bits) and then that many bytes' worth of words from `RDFD`.
- `recv(timeout)` polls `try_recv` until a packet arrives or the time is up.
- `recv_into` copies into a caller buffer. A packet longer than the buffer has
  already left the FIFO and cannot be put back, so it is discarded and the call
  fails with `BufferTooSmall { len, cap }`.
- Interrupt status/mask are exposed as raw `u32` bits; `set_interrupt_mask`
  chooses which flags drive the core's interrupt output, which the generated
  crate routes like any other PL interrupt
  ([Interrupts](../guide/interrupts.md)).

Not covered: the AXI4 full-data interface, cut-through mode, `TDEST`/`TID`/
`TUSER` sideband registers, and the receive-side partial-packet flags.
