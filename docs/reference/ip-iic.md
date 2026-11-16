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
configured. 10-bit addressing and slave mode are not supported. Verified on
hardware against a second IIC core acting as target ([G12](../08-verification-gaps.md)).

`IicError`: `Mmio`, `Nack`, `ArbitrationLost`, `Timeout`, `ReadTooLong(n)`.

**Status register note.** `ISR` is toggle-on-write (PG090): writing a 1 flips the bit. The driver clears it by writing back exactly the bits that are set. When used as a target, this core holds the bus until its RX FIFO is read, so drain it while the master transfer is in progress (the driver implements the master side only).
