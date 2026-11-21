# Video Timing Controller

`hardroxide_ip::vtc` — `xilinx.com:ip:v_tc` (PG016), generator side: the blanking, sync and active-video signals of a display mode.
The register values are those the vendor driver's `XVtc_SetGeneratorTiming` writes.

> **Not yet run on a board** (see [Verification gaps](../08-verification-gaps.md)); the register encoding is unit-tested.

```rust,ignore
pub struct Timing
pub const fn h_total(&self) -> u32
pub const fn v_total(&self) -> u32
pub struct VideoTimingController<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn reset(&self) -> Result<(), MmioError>
pub fn stop(&self) -> Result<(), MmioError>
pub fn start(&self, t: &Timing) -> Result<(), MmioError>
```

`Timing` holds `h_active`, `h_front_porch`, `h_sync`, `h_back_porch`, the same four for the vertical direction, and
`h_sync_positive` / `v_sync_positive`. `start` writes the generator registers (`GHSIZE` `0x70`, `GVSIZE` `0x74`, `GASIZE` `0x60` and
`0x94`, `GHSYNC` `0x78`, `GVSYNC` `0x80` and `0x8C`, the horizontal offsets of the vertical blank and sync `0x7C`, `0x84`, `0x88`,
`0x90`, and the polarity register `GPOL` `0x6C`), then the control register: every "take this value from the generator registers"
source bit, core enable, register update and generator enable. The vertical sync starts one line before the end of the front
porch in this core's numbering; each blank follows its sync's polarity; active video is active high. Interlaced modes are not supported.
