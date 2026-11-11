# Generated crate

## Layout

```text
Cargo.toml                 package + dependencies (hardroxide-* runtime crates)
interface.txt              canonical interface description (diffed by `update`)
interface.sha256           SHA-256 of interface.txt
bitstream/design.bin       raw configuration stream (byte-swapped, see below)
bitstream/design.sha256    SHA-256 of design.bin, checked at load
bitstream/load.cfg         ps_clk_hz and fclkN targets
interrupts.dtso            device-tree overlay exposing each routed PL interrupt as UIO
src/lib.rs                 Design, LoadOptions, Error
src/pac.rs                 register layer
src/ips.rs                 typed wrappers for HLS IP (only if the design has HLS IP)
examples/*.rs              smoke tests (only for the IP present)
```

`design.bin` is the `.bit` payload without its header (which contains build
timestamps), with each 32-bit word byte-swapped and padded with NOP words to a
multiple of 16 bytes. This is byte-identical to
`bootgen -arch zynq -process_bitstream bin` output.

Cargo features and dependencies:

| Item | Meaning |
|---|---|
| `dev-bitstream` (feature) | enables `Design::load_with_bitstream` (pulls in `hardroxide-hwh`) |
| `hardroxide-mmio`, `-fpga`, `-ip`, `-irq`, `-dma` | runtime |
| `embedded-hal` | dev-dependency, used by examples |

## `lib.rs`

```rust,ignore
pub const INTERFACE_HASH: &str = include_str!("../interface.sha256");
pub const BITSTREAM_SHA256: &str = include_str!("../bitstream/design.sha256");
pub fn embedded_load_config() -> LoadConfig
pub struct LoadOptions
pub struct Design<B: MmioBackend>
pub fn load() -> Result<Self, Error>
pub fn load_with(backend: B, opts: &LoadOptions) -> Result<Self, Error>
pub fn load_with_bitstream(xsa: impl AsRef<std::path::Path>, backend: B, opts: &LoadOptions) -> Result<Self, Error>
pub fn map_only(backend: B) -> Result<Self, Error>
```

- `Design<B>` has one `pub` field per addressable IP instance (named as in
  [Naming](../guide/generating.md#naming)), plus a private singleton guard.
  `Design<DevMem>::load()` is provided for the default backend.
- `LoadOptions { paths: FpgaPaths, apply_clocks: bool }`; the default uses the
  standard sysfs paths and applies clocks.
- `load_with` (and `load`, `load_with_bitstream`): takes the process-wide
  singleton, verifies the bitstream SHA-256, writes it through
  `fpga_manager`, applies the FCLK targets via the Zynq SLCR (if
  `apply_clocks`), then maps every IP. Any failure releases the singleton.
- `map_only` skips programming, clocks and the singleton.
- `load_with_bitstream` (feature `dev-bitstream`) compares the XSA's interface
  hash to `INTERFACE_HASH` and fails with `Error::InterfaceMismatch` if they
  differ.

### `Error`

| Variant | Cause |
|---|---|
| `AlreadyLoaded` | a `Design` loaded by `load*` already exists in this process |
| `Fpga(FpgaError)` | bitstream hash/programming or clock error |
| `Mmio(MmioError)` | mapping or access error |
| `Xsa(hardroxide_hwh::Error)` | (`dev-bitstream`) unreadable XSA |
| `InterfaceMismatch { found }` | (`dev-bitstream`) interface hash differs |

## `pac.rs`

One module per IP instance, with nested modules for registers and fields:

```rust,ignore,example
pub const VLNV: &str = "xilinx.com:ip:axi_gpio:2.0";
pub const BASE_ADDRESS: u64 = 0x41200000;
pub const SIZE: usize = 0x10000;
pub const INTERRUPTS: &[(&str, Option<u32>)] = &[("interrupt", Some(0))];
```

(The values above are examples.) For each connected interrupt output the
module also has `pub const IRQ_<PORT>: hardroxide_irq::IrqLine` and an
`IRQS: &[hardroxide_irq::IrqLine]` slice (see
[Interrupts](../guide/interrupts.md)). For each register `r` of the instance's first
address block: `pac::<inst>::<r>::OFFSET: usize` and `RESET_VALUE: u32`; for
each field `f`: `pac::<inst>::<r>::<f>::{SHIFT, WIDTH, MASK}` (`u32`).
An `IrqLine` whose signal feeds an `axi_intc` input has `via_intc:
Some((controller, bit))`, no `f2p_bit`/`gic_spi`, and no UIO node in the overlay
(use the controller's own `IRQ_IRQ`; see
[AXI Interrupt Controller](ip-intc.md)).
`INTERRUPTS` lists connected interrupt outputs as `(port, PS7 IRQ_F2P bit)`;
the bit is `None` if the net does not reach `IRQ_F2P` directly or through an
`xlconcat`. Only the first address range of an instance is described (see
[Verification gaps, D3](../08-verification-gaps.md)).

## `interrupts.dtso`

Present when at least one interrupt is connected. A device-tree overlay with
one `generic-uio` node per interrupt line (named `<instance>_<port>@<base>`),
whose `interrupts` cell is `<0 SPI trigger>` from the traced `IRQ_F2P` bit
(level-high unless the HWH says edge-rising) and `interrupt-parent = <&intc>`.
Interrupts that cannot be traced to `IRQ_F2P[15:0]` are listed in a comment and
skipped. Offline checks: it compiles with `dtc` and merges onto a
Zynq-7000-style base tree with `fdtoverlay` (a test does this when the tools
are installed). Whether the stock PYNQ image can apply and use it is
unverified ([G11](../08-verification-gaps.md)).

## Examples

| File | Present when | Checks |
|---|---|---|
| `board_check` | always | loads the bitstream, reads each FCLK back from the SLCR, opens every interrupt's UIO device; touches no IP register |
| `gpio_smoke` | AXI GPIO | walks a one-hot pattern on channel 1, prints channel 2 |
| `bram_test` | AXI BRAM Controller | writes a pattern over the whole block RAM and reads it back |
| `fifo_loopback` | AXI4-Stream FIFO | sends a 37-byte packet and reads it back (stream TX looped to RX) |
| `uart_hello` | AXI UART Lite | sends a greeting and prints what is received in one second |
| `hls_smoke` | HLS IP | prints idle/ready/done and readable registers; does not start the core |
| `dma_loopback` | AXI DMA | MM2S to S2MM loopback through two `u-dma-buf` devices |
| `timer_delay` | AXI Timer | a 100 ms hardware delay vs. the system clock |
| `spi_loopback` | AXI Quad SPI | internal-loopback transfer, no wiring needed |
| `i2c_scan` | AXI IIC | probes addresses `0x08..0x78` on a pulled-up bus |
| `irq_probe` | any routed interrupt | opens the UIO device of every interrupt line (checks the overlay); does not wait |

All require root on the board and have **not** been run on hardware.
