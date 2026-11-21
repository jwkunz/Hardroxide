# Dynamic clock generator (`axi_dynclk`)

`hardroxide_ip::dynclk` — Digilent's `digilentinc.com:ip:axi_dynclk`: a clock generator whose MMCM is reconfigured from software.
The PYNQ base overlay uses it for the HDMI output's pixel clock. The register map and the reconfiguration (divider encoding, lock
and filter tables from Xilinx XAPP888) follow Digilent's driver (MIT licence, github.com/Digilent/vivado-library), the only
description of this core; the code is written for this crate.

> **Not yet run on a board.** The register values are unit-tested (against the divider encoding and the MMCM's limits) and the
> sequence follows the vendor's; the first run on the base overlay is the check ([Verification gaps](../08-verification-gaps.md)).

```rust,ignore
pub struct Mode
pub struct Regs
pub fn registers_for(mode: &Mode) -> Option<Regs>
pub fn find_mode(freq_khz: u32, parent_khz: u32) -> Option<Mode>
pub enum DynclkError
pub struct AxiDynclk<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn ref_clk_hz(&self) -> Result<u32, MmioError>
pub fn locked(&self) -> Result<bool, MmioError>
pub fn disable(&self) -> Result<(), MmioError>
pub fn set_rate(&self, pixel_hz: u32) -> Result<u32, DynclkError>
```

Register map: `CTRL` `0x00` (1 = output on), `STATUS` `0x04` (non-zero = locked), the MMCM words `CLK_L` `0x08`, `FB_L` `0x0C`,
`FB_H_CLK_H` `0x10`, `DIV` `0x14`, `LOCK_L` `0x18`, `FLTR_LOCK_H` `0x1C`, and the reference clock frequency in Hz at `0x20` (read only).

- `set_rate(pixel_hz)` reads the reference clock, finds the MMCM settings for **five times** `pixel_hz` (the output goes through a
  divide-by-five buffer), writes the six words, switches the output off, waits for the old clock to stop, switches it on and
  waits for the lock (up to 100 ms, then `DynclkError::NoLock`). It returns the pixel frequency produced.
- `find_mode(freq_khz, parent_khz)` is the search: input divider upward from the smallest valid one, feedback multiplier over the
  range that keeps the VCO between 600 and 1200 MHz, output divider for the best match. It returns the first exact match or the
  closest one.
- `registers_for(&Mode)` encodes the settings (high/low times, the odd-divide edge bit, the lock and filter table entries).
