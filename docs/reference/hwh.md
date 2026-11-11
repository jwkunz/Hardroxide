# hardroxide-hwh

Parses an XSA archive and its Hardware Handoff (`.hwh`) into a typed model, and
converts the `.bit` to the `.bin` that `fpga_manager` loads. No Vivado needed.

## Opening an XSA

```rust,ignore
pub struct Xsa
pub design: Design,
pub bit: Vec<u8>,
pub fn open(path: impl AsRef<Path>) -> Result<Self, Error>
pub fn from_reader(r: impl Read + Seek) -> Result<Self, Error>
pub fn bin(&self) -> Result<Vec<u8>, Error>
pub fn parse_hwh(xml: &str) -> Result<Design, Error>
```

`Xsa` requires exactly one `.hwh` and one `.bit` entry (`Error::MissingEntry`
/ `Error::AmbiguousEntry` otherwise). `bit` holds the raw `.bit` bytes;
`bin()` is the loadable image. `parse_hwh` accepts the file text (a leading
BOM is tolerated).

## `Design`

```rust,ignore
pub struct Design
pub name: String,
pub vivado_version: String,
pub arch: String,
pub device: String,
pub package: String,
pub speed_grade: String,
pub instances: Vec<Instance>,
pub memory_ranges: Vec<MemRange>,
pub pl_clocks: Vec<PlClock>,
pub ps_clk_hz: u64,
pub fn instance(&self, name: &str) -> Option<&Instance>
pub fn ranges_of<'a>(&'a self, instance: &'a str) -> impl Iterator<Item = &'a MemRange>
pub fn enabled_fclks(&self) -> Vec<(u8, u64)>
pub fn ps7_irq_f2p_bit(&self, instance: &str, port: &str) -> Option<u32>
```

- `ps_clk_hz` is the PS reference clock (`PCW_CRYSTAL_PERIPHERAL_FREQMHZ`), `0`
  if absent.
- `pl_clocks` always has four entries (FCLK0..3); `enabled_fclks` returns
  `(index, Hz)` for the enabled ones.
- `ps7_irq_f2p_bit(instance, port)` follows the net from a 1-bit output port
  to the PS7 `IRQ_F2P` vector, through any chain of `xlconcat` blocks (using
  their `INk_WIDTH` parameters). Returns `None` for wider ports, unconnected
  or fanned-out nets, depth beyond 8, or other logic in between.

## Model types

```rust,ignore
pub struct Instance
pub name: String,
pub vlnv: String,
pub mod_type: String,
pub is_ps: bool,
pub parameters: BTreeMap<String, String>,
pub address_blocks: Vec<AddressBlock>,
pub interrupts: Vec<InterruptPort>,
pub ports: Vec<Port>,
pub fn axi_clk_hz(&self) -> Option<u64>
pub fn is_hls(&self) -> bool
pub struct Port
pub dir: String,
pub sinks: Vec<(String, String)>,
pub clk_hz: Option<u64>,
pub struct InterruptPort
pub signal: String,
pub sensitivity: String,
pub struct AddressBlock
pub interface: String,
pub range: u64,
pub usage: String,
pub registers: Vec<Register>,
pub struct Register
pub description: String,
pub offset: u64,
pub size: u32,
pub access: Access,
pub reset_value: u64,
pub fields: Vec<Field>,
pub struct Field
pub bit_offset: u32,
pub bit_width: u32,
pub enum Access
pub struct MemRange
pub address_block: String,
pub slave_interface: String,
pub master_interface: String,
pub base: u64,
pub high: u64,
pub fn size(&self) -> u64
pub struct PlClock
pub index: u8,
pub enabled: bool,
pub freq_hz: u64,
pub reset_enabled: bool,
```

Conventions and quirks (see [HWH findings](../07-hwh-findings.md)):

- `MemRange::high` is **inclusive**; `size()` is `high - base + 1`.
- `Register::offset` is relative to its address block. `size` is a hint only:
  Vivado is inconsistent (AXI GPIO data registers report their field width).
- `Access` is `ReadWrite`, `ReadOnly`, `WriteOnly` or `Other` (anything else).
- `is_hls()` is true when the VLNV library field is `hls`.
- `axi_clk_hz()` returns the frequency of `s_axi_aclk`, else of the first port
  that reports one.
- Collections keep document order, except `parameters` (sorted by name).

## Interrupt numbering

```rust,ignore
pub fn zynq7000_f2p_spi_cell(bit: u32) -> Option<u32>
```

Maps a PS7 `IRQ_F2P` bit to the Zynq-7000 GIC SPI *device-tree cell*
(`interrupts = <0 N 4>`): bits 0..=7 give 29..=36 (interrupt IDs 61..=68) and
bits 8..=15 give 52..=59 (IDs 84..=91); `None` above 15. From UG585 table 7-4,
unverified on hardware ([G11](../08-verification-gaps.md)).
`InterruptPort::sensitivity` is the HWH `SENSITIVITY` attribute (for example
`LEVEL_HIGH`), empty if absent.

## Interface hash

```rust,ignore
pub fn interface_text(&self) -> String
pub fn interface_hash(&self) -> String
```

`interface_text` is a canonical description of the PL's *interface*:
non-PS instances sorted by name with VLNV, address blocks, registers
(name, offset, access, reset value), fields, interrupt ports with their sinks,
and the address ranges. It excludes IP parameters, descriptions, clocks, PS
configuration and timestamps. `interface_hash` is its lower-case hex SHA-256.
This is what `hardroxide update` compares.

## `.bit` handling

```rust,ignore
pub struct BitFile<'a>
pub design_name: String,
pub part: String,
pub date: String,
pub time: String,
pub payload: &'a [u8],
pub fn parse(data: &'a [u8]) -> Result<Self, BitError>
pub fn to_bin_swapped(&self) -> Result<Vec<u8>, BitError>
```

`parse` reads the Xilinx container header (the `date`/`time` fields are the
build timestamps and are never embedded). `to_bin_swapped` byte-swaps each
32-bit word and pads with `0x20000000` NOP words to 16 bytes, matching bootgen
(verified byte-for-byte on the fixtures).

## Errors

`Error`: `Io`, `Zip`, `MissingEntry(ext)`, `AmbiguousEntry(ext)`, `Xml`,
`Hwh(String)` (structurally invalid HWH), `Bit(BitError)`. `BitError`:
`Truncated`, `BadTag(u8)`, `Unaligned(len)`.
