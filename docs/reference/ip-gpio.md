# AXI GPIO

`hardroxide_ip::gpio` — `xilinx.com:ip:axi_gpio` (PG144). Two channels, each
with a data register and a tri-state register.

```rust,ignore
pub enum Channel
pub struct AxiGpio<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn set_input_mask(&self, ch: Channel, mask: u32) -> Result<(), MmioError>
pub fn input_mask(&self, ch: Channel) -> Result<u32, MmioError>
pub fn write(&self, ch: Channel, value: u32) -> Result<(), MmioError>
pub fn read(&self, ch: Channel) -> Result<u32, MmioError>
pub fn enable_interrupt(&self, ch: Channel) -> Result<(), MmioError>
pub fn disable_interrupt(&self, ch: Channel) -> Result<(), MmioError>
pub fn interrupt_pending(&self, ch: Channel) -> Result<bool, MmioError>
pub fn clear_interrupt(&self, ch: Channel) -> Result<(), MmioError>
pub fn into_inner(self) -> R
```

`Channel` is `One` or `Two`. Register map: channel 1 data `0x0`, tri `0x4`;
channel 2 data `0x8`, tri `0xC`.

- `set_input_mask(ch, mask)`: a `1` bit makes the pin an **input**, `0` an
  output (the register is a tri-state control). The reset value is all inputs
  unless the IP was configured "all outputs".
- `write` drives output pins; bits for input pins are ignored by the hardware.
- `read` returns pin levels; outputs read back the driven value.
- Pin width is not checked: bits above the configured width are meaningless
  (the HWH parameters `C_GPIO_WIDTH` / `C_GPIO2_WIDTH` give the widths).
- Interrupts (the IP must be built with its interrupt enabled): `enable_interrupt(ch)` sets the global
  enable (`GIER` at `0x11C`) and the channel's bit in `IP_IER` (`0x128`); the interrupt fires when any
  pin of that channel changes. `interrupt_pending(ch)` reads `IP_ISR` (`0x120`), and `clear_interrupt(ch)`
  clears it. `IP_ISR` **toggles** on a write of 1, so `clear_interrupt` writes only while the bit is
  set. The level-sensitive output stays asserted until cleared, so clear the GPIO's flag before
  acknowledging an interrupt controller input it feeds.
