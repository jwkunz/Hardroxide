# AXI Timer

`hardroxide_ip::timer` — `xilinx.com:ip:axi_timer` (PG079), timer 0 in
generate mode, counting down.

```rust,ignore
pub const TCSR0: usize = 0x00;
pub const TLR0: usize = 0x04;
pub const TCR0: usize = 0x08;
pub struct AxiTimer<R: Mmio>
pub fn new(regs: R, clk_hz: u64) -> Self
pub fn clock_hz(&self) -> u64
pub fn regs(&self) -> &R
pub fn start(&self, ticks: u32, periodic: bool, irq: bool) -> Result<(), MmioError>
pub fn stop(&self) -> Result<(), MmioError>
pub fn counter(&self) -> Result<u32, MmioError>
pub fn expired(&self) -> Result<bool, MmioError>
pub fn clear(&self) -> Result<(), MmioError>
pub fn ticks_for_ns(&self, ns: u64) -> u64
pub fn delay_ticks(&self, mut ticks: u64) -> Result<(), MmioError>
impl<R: Mmio> DelayNs for AxiTimer<R>
```

- `new(regs, clk_hz)`: `clk_hz` is the AXI clock driving the counter (the
  generated `Design` passes the HWH clock-port frequency, or `0` if unknown).
- `start` loads `ticks`, sets down-count, optionally auto-reload (`periodic`)
  and the interrupt enable, clears any stale expiry flag and starts the timer.
- `expired` reports the `TINT` flag without clearing it; `clear` clears it
  (write-1-to-clear). `counter` reads the current count.
- `ticks_for_ns` rounds up.
- `delay_ticks` busy-waits, splitting long delays into 32-bit chunks; the
  smallest hardware delay is two ticks.
- `DelayNs` (`embedded-hal` 1.0) is implemented on top of `delay_ticks`; as
  `DelayNs` cannot return errors, an MMIO failure **panics** there. Use
  `delay_ticks` to handle errors.

Timer 1 (cascade/PWM/capture modes) is not wrapped.
