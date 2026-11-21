# Dynamic clock generator (`axi_dynclk`)

`hardroxide_ip::dynclk` — Digilent's `digilentinc.com:ip:axi_dynclk`: a clock generator whose MMCM is reconfigured from software.
The PYNQ base overlay uses it for the HDMI output's pixel clock. The register map and the reconfiguration (divider encoding, lock
and filter tables from Xilinx XAPP888) follow Digilent's driver (MIT licence, github.com/Digilent/vivado-library), the only
description of this core; the code is written for this crate.

> **Verified on a PYNQ-Z2:** the words written for 148.5 MHz are byte-identical to what Linux's driver programs (`LOCK_L` `0xCFAFA401`, `FLTR_LOCK_H` `0x00A300FF`), and pixel clocks from 40 to 148.5 MHz drive the HDMI output ([Verification gaps](../08-verification-gaps.md)). 25.2 MHz locks but the tested monitor rejects the 640x480 signal.

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
pub fn set_rate(&self, ref_hz: u32, pixel_hz: u32) -> Result<u32, DynclkError>
```

Register map: `CTRL` `0x00` (1 = output on), `STATUS` `0x04` (non-zero = locked), the MMCM words `CLK_L` `0x08`, `FB_L` `0x0C`,
`FB_H_CLK_H` `0x10`, `DIV` `0x14`, `LOCK_L` `0x18`, `FLTR_LOCK_H` `0x1C`, and, in newer versions of the core only, the reference clock frequency in Hz at `0x20`. **The core in the PYNQ base overlay is the older one with eight registers** (found on the board: the register at `0x20` reads 0), so the reference, FCLK0 at 100 MHz on `REF_CLK_I`, is passed by the caller.

- `set_rate(ref_hz, pixel_hz)` takes the reference clock, finds the MMCM settings for **five times** `pixel_hz` (the output goes through a
  divide-by-five buffer), writes the six words, switches the output off, waits for the old clock to stop, switches it on and
  waits for the lock (up to 100 ms, then `DynclkError::NoLock`). It returns the pixel frequency produced.
- `find_mode(freq_khz, parent_khz)` is the search: input divider upward from the smallest valid one, feedback multiplier over the
  range that keeps the VCO between 600 and 1200 MHz, output divider for the best match. It returns the first exact match or the
  closest one.
- `registers_for(&Mode)` encodes the settings (high/low times, the odd-divide edge bit, the lock and filter table entries).
