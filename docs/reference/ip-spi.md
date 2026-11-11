# AXI Quad SPI

`hardroxide_ip::spi` — `xilinx.com:ip:axi_quad_spi` (PG153) in **standard SPI
mode** as a master, with manual slave select.

```rust,ignore
pub struct AxiSpi<R: Mmio>
pub fn new(regs: R, fifo_depth: usize, num_ss: usize) -> Self
pub fn regs(&self) -> &R
pub fn init(&mut self, mode: Mode, lsb_first: bool, loopback: bool) -> Result<(), SpiError>
pub fn set_chip_select(&mut self, cs: Option<usize>) -> Result<(), SpiError>
pub fn device(&mut self, cs: usize) -> Result<SpiSlave<'_, R>, SpiError>
impl<R: Mmio> SpiBus<u8> for AxiSpi<R>
impl<R: Mmio> SpiDevice<u8> for SpiSlave<'_, R>
```

- `new(regs, fifo_depth, num_ss)`: from the HWH (`C_FIFO_DEPTH`, or 1 if the IP
  has no FIFOs; `C_NUM_SS_BITS`). The generated `Design` fills these in.
- `init(mode, lsb_first, loopback)` resets the core and configures master mode,
  manual slave select, the `embedded-hal` SPI `Mode` (CPOL/CPHA), bit order,
  and optionally the internal loopback bit (data returns without any wiring).
  The SCK rate is fixed by the IP (`C_SCK_RATIO`) and cannot be set here.
- `set_chip_select(Some(n))` selects which slave line `SpiBus` transfers
  assert (`None`: none). `SpiError::BadChipSelect(n)` if `n >= num_ss`.
- `SpiBus<u8>`: `read` clocks out zeros; `transfer` pads the shorter buffer
  with zeros; `flush` is a no-op because every call completes before
  returning. Transfers are chunked to the FIFO depth.
- `device(cs)` returns a `SpiDevice<u8>` that asserts slave line `cs` for
  each operation. **Limitation:** chip select is released between the
  operations of one `transaction`, so a protocol needing CS held across
  several operations is not supported yet ([G12](../08-verification-gaps.md)).
  `Operation::DelayNs` sleeps the thread.

`SpiError`: `Mmio`, `Timeout`, `BadChipSelect(n)`. All errors map to
`ErrorKind::Other`. Quad-SPI/XIP modes are not supported. **Unverified on
hardware.**
