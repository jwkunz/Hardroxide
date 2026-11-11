# HLS IP

`hardroxide_ip::hls` — control helpers for Vitis HLS cores with an AXI-Lite
control interface. Layout (UG1399): `0x00` CTRL, `0x04` GIER, `0x08` IP_IER,
`0x0C` IP_ISR, user registers from `0x10`.

```rust,ignore
pub const CTRL: usize = 0x00;
pub const GIER: usize = 0x04;
pub const IP_IER: usize = 0x08;
pub const IP_ISR: usize = 0x0C;
pub const USER_BASE: usize = 0x10;
pub struct HlsIp<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn start(&self) -> Result<(), MmioError>
pub fn is_idle(&self) -> Result<bool, MmioError>
pub fn is_ready(&self) -> Result<bool, MmioError>
pub fn is_done(&self) -> Result<bool, MmioError>
pub fn set_auto_restart(&self, on: bool) -> Result<(), MmioError>
pub fn wait_done(&self, timeout: Duration) -> Result<(), HlsError>
pub fn enable_done_interrupt(&self) -> Result<(), MmioError>
pub fn disable_interrupts(&self) -> Result<(), MmioError>
pub fn ack_interrupts(&self) -> Result<u32, MmioError>
```

- `start` sets `ap_start`, preserving `auto_restart`. It first **reads** CTRL,
  which clears the read-to-clear `ap_done`/`ap_ready` bits (as Xilinx's own
  driver does).
- `is_done` / `is_ready` clear on read: a `true` is consumed.
- `wait_done` polls `ap_done`; `HlsError::Timeout(d)` on expiry, or
  `HlsError::Mmio`.
- `enable_done_interrupt` sets GIER bit 0 and IP_IER bit 0.
- `ack_interrupts` writes back the pending bits of IP_ISR (toggle-on-write)
  and returns which were pending (`bit0` = done, `bit1` = ready).

## Generated typed wrappers

For each HLS instance the generator emits (in `src/ips.rs`) a struct that
wraps `HlsIp` and **derefs to it**, so `start()`, `is_done()`, … are available
directly. Every register at offset `0x10` or above becomes methods named after
the register (lower-cased and sanitised, same rules as the
[PAC](generated-crate.md#pacrs)):

- readable registers (access not write-only): `fn <reg>(&self) -> Result<u32, MmioError>`
- writable registers (access not read-only): `fn set_<reg>(&self, value: u32) -> Result<(), MmioError>`

Example for the `vadd` fixture (`a`, `b` write-only, `out_r`, `out_r_ctrl`
read-only): `set_a`, `set_b`, `out_r`, `out_r_ctrl`. Registers are exposed as
raw 32-bit words: 64-bit values and arrays are not assembled for you.
