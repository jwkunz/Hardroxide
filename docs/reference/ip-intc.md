# AXI Interrupt Controller

`hardroxide_ip::intc` — `xilinx.com:ip:axi_intc` (PG099) used as a fan-in:
several PL interrupts share the controller's single `irq` output, which is the
only interrupt that reaches the PS and so the only one with a UIO device. The
driver tells you which input fired. Only software-driven use is covered (no
fast-interrupt or cascade modes).

```rust,ignore
pub const ISR: usize = 0x00;
pub const IPR: usize = 0x04;
pub const IER: usize = 0x08;
pub const IAR: usize = 0x0C;
pub const SIE: usize = 0x10;
pub const CIE: usize = 0x14;
pub const IVR: usize = 0x18;
pub const MER: usize = 0x1C;
pub enum IntcError
pub struct AxiIntc<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn init(&self) -> Result<(), IntcError>
pub fn enable(&self, mask: u32) -> Result<(), IntcError>
pub fn disable(&self, mask: u32) -> Result<(), IntcError>
pub fn enabled(&self) -> Result<u32, IntcError>
pub fn pending(&self) -> Result<u32, IntcError>
pub fn highest_pending(&self) -> Result<Option<u32>, IntcError>
pub fn ack(&self, mask: u32) -> Result<(), IntcError>
pub fn wait<W: IrqWaiter>(
```

Bit `n` of every mask is controller input `n`, i.e. the `bit` in an
[`IrqLine::via_intc`](irq.md#interrupt-lines-and-uio-discovery) of a device
that feeds it.

- `init` disables and clears all inputs and sets `MER.ME | MER.HIE` so the
  `irq` output is live.
- `enable`/`disable` use the atomic `SIE`/`CIE` registers.
- `pending` is `IPR` (latched **and** enabled). `highest_pending` reads `IVR`
  (the lowest-numbered pending input, `None` when the register reads
  `0xFFFF_FFFF`).
- `ack` writes `IAR`. For a level-sensitive input (the usual case: Timer, IIC,
  GPIO, DMA), **clear the source device first**, then `ack`, or the input
  re-latches immediately.
- `wait(irq, mask, timeout)` unmasks the UIO line, checks `pending() & mask`,
  and otherwise blocks on `irq` (any [`IrqWaiter`](irq.md#waiting), in practice
  a `UioIrq` opened from the controller's own `IRQ_IRQ` constant). It returns
  the pending bits inside `mask`, or `None` on timeout, and does *not*
  acknowledge. Events for inputs outside `mask` are ignored and the wait
  continues with the remaining time.

Per-input edge/level is fixed at build time (`C_KIND_OF_INTR`); it is not
configurable from software and the driver does not read it.
