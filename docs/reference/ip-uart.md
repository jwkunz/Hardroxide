# AXI UART Lite

`hardroxide_ip::uart` — `xilinx.com:ip:axi_uartlite` (PG142): a serial port
with 16-byte FIFOs. Baud rate, data bits and parity are fixed when the
bitstream is built; nothing is configurable from software.

```rust,ignore
pub const RX_FIFO: usize = 0x00;
pub const TX_FIFO: usize = 0x04;
pub const STAT_REG: usize = 0x08;
pub const CTRL_REG: usize = 0x0C;
pub enum UartError
pub struct RxErrors
pub struct AxiUart<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn init(&self) -> Result<(), UartError>
pub fn set_interrupts(&self, on: bool) -> Result<(), UartError>
pub fn errors(&self) -> Result<RxErrors, UartError>
pub fn tx_empty(&self) -> Result<bool, UartError>
pub fn try_write_byte(&self, b: u8) -> Result<bool, UartError>
pub fn try_read_byte(&self) -> Result<Option<u8>, UartError>
pub fn send(&self, data: &[u8], timeout: Duration) -> Result<(), UartError>
pub fn receive(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, UartError>
impl<R: Mmio> std::io::Write for AxiUart<R>
```

- `init` resets both FIFOs. `set_interrupts(true)` raises the core's interrupt
  on received data and when the TX FIFO empties (it is routed like any other
  PL interrupt; see [Interrupts](../guide/interrupts.md)).
- `try_write_byte` returns `false` when the TX FIFO is full; `try_read_byte`
  returns `None` when nothing is waiting.
- `send` busy-waits for FIFO space and fails with `UartError::Timeout` after
  `timeout`. `receive` returns as soon as `buf` is full or `timeout` has
  elapsed in total, with however many bytes arrived (possibly 0).
- `errors` returns the overrun/frame/parity flags and **clears them** (reading
  the status register clears the error bits).
- `std::io::Write` sends with a 1 s timeout; `flush` waits for the TX FIFO to
  drain. There is no `std::io::Read` because "no data yet" is not end of file;
  use `receive`.
