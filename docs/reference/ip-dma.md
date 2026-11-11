# AXI DMA

`hardroxide_ip::dma` — `xilinx.com:ip:axi_dma` (PG021) in **direct register
(simple) mode**. Scatter-gather is not supported.

```rust,ignore
pub enum Direction
pub struct AxiDma<R: Mmio>
pub fn with_length_width(regs: R, length_width: u32) -> Self
pub fn new(regs: R) -> Self
pub fn with_interrupts(mut self, on: bool) -> Self
pub fn max_transfer_len(&self) -> usize
pub fn regs(&self) -> &R
pub fn reset(&self) -> Result<(), DmaIpError>
pub fn start<B: DmaBuffer>(&self, dir: Direction, buf: B, len: usize) -> Result<Transfer<'_, R, B>, StartError<B>>
pub struct Transfer<'d, R: Mmio, B: DmaBuffer>
pub fn poll(&self) -> Result<bool, DmaIpError>
pub fn wait(mut self, timeout: Duration) -> Result<B, DmaIpError>
pub struct StartError<B>
pub error: DmaIpError,
pub buffer: B,
```

`Direction` is `MemToStream` (MM2S: the PL reads the buffer) or `StreamToMem`
(S2MM: the PL writes it).

- `with_length_width(regs, w)`: `w` is the IP's buffer-length-register width
  (HWH `C_SG_LENGTH_WIDTH`); the maximum transfer is `2^w - 1` bytes. `new`
  assumes the Vivado default of 14.
- `with_interrupts(true)` sets the IOC and error interrupt enables for
  subsequent transfers.
- `reset()` soft-resets both channels and waits for the reset bits to clear.
- `start` validates `0 < len <= min(buffer length, max_transfer_len())` and
  that the channel is halted or idle (`DmaIpError::Busy`), enables the channel,
  clears stale status, programs the address and finally LENGTH (which starts
  the transfer). On failure the buffer is returned inside `StartError`.

## Ownership of the buffer

`start` **moves** the buffer into the returned `Transfer`, so the CPU cannot
touch memory the hardware may still be accessing.

- `poll()`: `Ok(true)` when the transfer completed; `Err(Hardware(sr))` if an
  error flag (DMAIntErr/DMASlvErr/DMADecErr) is set.
- `wait(timeout)`: polls to completion, clears the status flags and returns
  the buffer. After an interrupt, `wait(Duration::ZERO)` collects the result.
  On error or timeout the channel is **halted first**, then the error is
  returned and the buffer dropped.
- Dropping an in-flight `Transfer` halts the channel before the buffer is
  freed. If the channel cannot be confirmed halted, the buffer is **leaked**
  (`DmaIpError::StuckChannel`) so hardware can never write freed memory.

For a stream loopback, arm the receiver (`StreamToMem`) before starting the
sender. Register map and bit positions are asserted against a real HWH; real
behaviour is unverified ([G10](../08-verification-gaps.md)).

`DmaIpError`: `Mmio`, `BadLength { len, max, buf }`, `Busy(sr)`,
`Hardware(sr)`, `Timeout(d)`, `StuckChannel`.
