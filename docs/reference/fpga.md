# hardroxide-fpga

Programs the PL and configures its clocks.

## Features and `no_std`

The `std` feature (default) provides the Linux loader below. Without it the crate is
`#![no_std]` and offers only the [SLCR clock code](#pl-clocks), which bare-metal programs
use; `FpgaError` then has only the clock and register variants.

## Loading a bitstream

```rust,ignore
pub fn load_bitstream(paths: &FpgaPaths, bin: &[u8], expected_sha256: &str) -> Result<(), FpgaError>
pub fn sha256_hex(data: &[u8]) -> String
pub struct FpgaPaths
pub manager: PathBuf,
pub firmware_dir: PathBuf,
pub platform_devices: PathBuf,
pub refuse_bound_pl_drivers: bool,
pub fn pl_drivers_in_use(platform_devices: &Path) -> Result<Vec<String>, FpgaError>
```

`load_bitstream` (1) checks `sha256_hex(bin) == expected_sha256`
(`FpgaError::HashMismatch`), (1b) if `refuse_bound_pl_drivers` (default `true`) fails with `FpgaError::PlInUse` when any platform device at a PL address (`0x4000_0000..=0xBFFF_FFFF`) has a driver bound other than the passive `uio_pdrv_genirq` (`pl_drivers_in_use` scans `platform_devices`, default `/sys/bus/platform/devices`; a missing directory means none; see [G16](../08-verification-gaps.md): reprogramming under such a driver froze a board), (2) writes the image to
`<firmware_dir>/hardroxide-<hash16>.bin`, (3) writes `0` to
`<manager>/flags` (full reconfiguration), (4) writes the file name to
`<manager>/firmware`, (5) removes the file and requires `<manager>/state` to
read `operating` (`FpgaError::NotOperating`). `FpgaPaths::default()` is
`/sys/class/fpga_manager/fpga0`, `/lib/firmware` and `/sys/bus/platform/devices`; override for tests with a
fake tree. Requires root. Verified on a PYNQ-Z2 ([G1](../08-verification-gaps.md)).

## Load configuration

```rust,ignore
pub struct LoadConfig
pub ps_clk_hz: u64,
pub fclks: Vec<(u8, u64)>,
pub fn to_text(&self) -> String
pub fn parse(text: &str) -> Result<Self, String>
pub fn apply(&self, slcr: &impl hardroxide_mmio::Mmio) -> Result<(), FpgaError>
```

The text form (`bitstream/load.cfg`) is one `key=value` per line:
`ps_clk_hz=50000000`, `fclk0=100000000` (indices 0..=3). `parse` ignores blank
lines and `#` comments and rejects unknown keys. `apply` sets every configured
FCLK through [`slcr::set_pl_clock`](#pl-clocks).

## PL clocks

```rust,ignore
pub const SLCR_BASE: u64 = 0xF800_0000;
pub const SLCR_SIZE: usize = 0x1000;
pub fn set_pl_clock(slcr: &impl Mmio, ps_clk_hz: u64, index: u8, hz: u64) -> Result<u64, FpgaError>
pub fn read_pl_clock(slcr: &impl Mmio, ps_clk_hz: u64, index: u8) -> Result<u64, FpgaError>
pub fn best_divisors(src_hz: u64, target_hz: u64) -> (u32, u32, u64)
pub fn apply_clocks(slcr: &impl Mmio, ps_clk_hz: u64, fclks: &[(u8, u64)]) -> Result<(), FpgaError>
```

(In module `hardroxide_fpga::slcr`.) `set_pl_clock` reads the FCLK's source
select and the corresponding PLL feedback divider, computes the source
frequency as `ps_clk_hz * fdiv`, picks two 6-bit divisors (1..=63) minimising
the error, and writes them. It accepts up to 1% error and returns the achieved
frequency. The SLCR is unlocked for the write and relocked afterwards. Errors:
`BadClockIndex`, `PllNotRunning` (zero divider or bypass), `ClockUnreachable`.
`read_pl_clock` is the read-only inverse: it computes the current FCLK
frequency from the same registers (`0` if a divisor is zero), which the
generated `board_check` example uses to compare against the design.
`best_divisors` returns `(div0, div1, achieved_hz)`; ties prefer the smaller
`div1`. The register layout is transcribed from the Zynq-7000 TRM and verified on a
PYNQ-Z2 ([G2](../08-verification-gaps.md)). PL resets are not handled
([G3](../08-verification-gaps.md)).

`FpgaError` variants: `HashMismatch`, `Io`, `NotOperating`, `BadClockIndex`,
`ClockUnreachable`, `PllNotRunning`, `Mmio`.

## Zynq UltraScale+ (`zynqmp`)

New in 3.0, run on an RFSoC 4x2 (PYNQ 4.0.0) with seven designs. The register values and the loading steps are those
PYNQ uses; the bitstream format is checked against `bootgen`.

```rust,ignore
pub const CRL_APB_BASE: u64 = 0xFF5E_0000;
pub const CRF_APB_BASE: u64 = 0xFD1A_0000;
pub fn read_pl_clock(
pub fn set_pl_clock(
pub enum Port
pub fn exists(self) -> bool
pub enum Width
pub fn from_bits(bits: u32) -> Option<Self>
pub fn bits(self) -> u32
pub fn set_port_width(
pub struct PortConfig
pub fn to_text(&self) -> String
pub fn parse(text: &str) -> Result<Self, String>
pub fn apply(&self, backend: &impl MmioBackend) -> Result<(), FpgaError>
pub const PL_ADDRESSES: [RangeInclusive<u64>; 3]
```

- `read_pl_clock` and `set_pl_clock(crl, crf, ps_ref_hz, index, ...)` are this family's `pl_clk0..3`, through the two
  clock-register blocks; setting keeps the clock's source and switches it on, within 1 %.
- `Port::Master(0..=2)` is `HPM0_FPD`, `HPM1_FPD`, `HPM0_LPD`; `Port::Slave(0..=6)` is `S_AXI_GP0..6`. A boot loader sets
  their widths for its own design, so a bitstream loaded later needs its widths applied; `set_port_width` writes the
  registers PYNQ writes.
- `zynqmp::load_bitstream` is `load_bitstream` with this family's PL addresses (`PL_ADDRESSES`) for the check of bound
  drivers (std).
- `PortConfig` is a generated crate's `bitstream/ports.cfg` (`m0=128`, `s2=64`), std.

