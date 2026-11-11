# AXI IIC

`hardroxide_ip::iic` — `xilinx.com:ip:axi_iic` (PG090) as an I2C master using
the dynamic controller logic (START/STOP flagged in TX FIFO words).

```rust,ignore
pub struct AxiIic<R: Mmio>
pub fn new(regs: R) -> Self
pub fn set_timeout(&mut self, t: Duration)
pub fn regs(&self) -> &R
pub fn init(&self) -> Result<(), IicError>
pub fn transaction_ops(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), IicError>
impl<R: Mmio> i2c::I2c for AxiIic<R>
```

- `init()` soft-resets the core, flushes the TX FIFO, enables it and clears
  the interrupt status. Call it once before use.
- `set_timeout` bounds every FIFO wait (default 100 ms).
- `transaction_ops` (also reached through `embedded_hal::i2c::I2c`) runs a
  sequence of read/write operations as one transaction: a START and address
  begins the first operation and every direction change (repeated start);
  consecutive operations in the same direction are merged; STOP is flagged on
  the last byte of the last operation. Reads are limited to 255 bytes per
  operation (`IicError::ReadTooLong`). An empty trailing write sends START|STOP
  on the address byte, and empty reads are skipped.
- Completion waits for the TX FIFO to empty and the bus to go idle.
- Errors map to `embedded-hal` kinds: `Nack` to `NoAcknowledge(Unknown)`,
  `ArbitrationLost` to `ArbitrationLoss`, everything else to `Other`. After a
  NACK or arbitration loss the core is reset.

Bus speed is whatever the IP's timing registers default to; they are not
configured. 10-bit addressing and slave mode are not supported. **Unverified
on hardware** ([G12](../08-verification-gaps.md)).

`IicError`: `Mmio`, `Nack`, `ArbitrationLost`, `Timeout`, `ReadTooLong(n)`.
