# AXI GPIO

`hardroxide_ip::gpio` — `xilinx.com:ip:axi_gpio` (PG144). Two channels, each
with a data register and a tri-state register.

```rust,ignore
pub enum Channel
pub struct AxiGpio<R: Mmio>
pub fn new(regs: R) -> Self
pub fn set_input_mask(&self, ch: Channel, mask: u32) -> Result<(), MmioError>
pub fn input_mask(&self, ch: Channel) -> Result<u32, MmioError>
pub fn write(&self, ch: Channel, value: u32) -> Result<(), MmioError>
pub fn read(&self, ch: Channel) -> Result<u32, MmioError>
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
- Interrupt registers (GIER/IP_IER/IP_ISR) are not wrapped; use `regs()`-level
  access via the generated [`pac`](generated-crate.md#pacrs) offsets.
