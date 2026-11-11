# hardroxide-fpga

Programs the PL and configures its clocks.

## Loading a bitstream

```rust,ignore
pub fn load_bitstream(paths: &FpgaPaths, bin: &[u8], expected_sha256: &str) -> Result<(), FpgaError>
pub fn sha256_hex(data: &[u8]) -> String
pub struct FpgaPaths
pub manager: PathBuf,
pub firmware_dir: PathBuf,
```

`load_bitstream` (1) checks `sha256_hex(bin) == expected_sha256`
(`FpgaError::HashMismatch`), (2) writes the image to
`<firmware_dir>/hardroxide-<hash16>.bin`, (3) writes `0` to
`<manager>/flags` (full reconfiguration), (4) writes the file name to
`<manager>/firmware`, (5) removes the file and requires `<manager>/state` to
read `operating` (`FpgaError::NotOperating`). `FpgaPaths::default()` is
`/sys/class/fpga_manager/fpga0` and `/lib/firmware`; override for tests with a
fake tree. Requires root. **Unverified on hardware** ([G1](../08-verification-gaps.md)).

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
pub fn best_divisors(src_hz: u64, target_hz: u64) -> (u32, u32, u64)
```

(In module `hardroxide_fpga::slcr`.) `set_pl_clock` reads the FCLK's source
select and the corresponding PLL feedback divider, computes the source
frequency as `ps_clk_hz * fdiv`, picks two 6-bit divisors (1..=63) minimising
the error, and writes them. It accepts up to 1% error and returns the achieved
frequency. The SLCR is unlocked for the write and relocked afterwards. Errors:
`BadClockIndex`, `PllNotRunning` (zero divider or bypass), `ClockUnreachable`.
`best_divisors` returns `(div0, div1, achieved_hz)`; ties prefer the smaller
`div1`. The register layout is transcribed from the Zynq-7000 TRM and
**unverified on hardware** ([G2](../08-verification-gaps.md)). PL resets are
not handled ([G3](../08-verification-gaps.md)).

`FpgaError` variants: `HashMismatch`, `Io`, `NotOperating`, `BadClockIndex`,
`ClockUnreachable`, `PllNotRunning`, `Mmio`.
