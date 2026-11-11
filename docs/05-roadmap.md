# 05 - Roadmap and Policies

Status: draft, updated through interview round 6.

## Policies

| Topic | Decision |
|---|---|
| Rust version | Latest stable, edition 2024. No fixed MSRV yet; revisit before first publish. |
| HLS / custom IP | Generate per-IP structs with named registers from the HWH register map, plus `ap_ctrl` start/done/idle helpers when the control protocol is detected. |
| License | `MIT OR Apache-2.0` |

## Milestone 1: GPIO end-to-end on PYNQ-Z2

Success criteria:

1. Parse a GPIO-only XSA (Vivado 2025.2) with `hardroxide-hwh`.
2. `hardroxide generate` emits a crate with the embedded `.bin`, register layer,
   and a `Design` struct.
3. `Design::load()` programs the PL, applies FCLK settings from the HWH, maps
   the GPIO via the `/dev/mem` backend.
4. Example binary blinks the PYNQ-Z2 LEDs and reads buttons on real hardware.
5. Golden-fixture snapshot test and mock-MMIO unit test pass in CI without hardware.

## Later milestones

2. `hardroxide update` and interface/bitstream hashing, `dev-bitstream` loader. **Done offline** (08, D4).
3. HLS / custom IP generation from HWH register maps. **Done offline**: scalar
   registers (vadd fixture) and 64-bit pointer arguments with buffer setters
   (scale fixture) (08, G9).
4. AXI DMA with uncached `DmaBuffer`, UIO interrupts (sync, then async feature).
   **Done offline** (08, G10/G11): `IrqLine` constants, `interrupts.dtso`,
   `irq_probe`, and fan-in through an AXI interrupt controller (G13).
5. AXI Timer, IIC, SPI with `embedded-hal` trait impls. **Done offline** (08, G12).
6. More catalog IP: AXI BRAM Controller, UART Lite, AXI4-Stream FIFO. **Done
   offline** (08, G14/G15).
7. Cached DMA backend, then partial reconfiguration and UltraScale+ support. Not started.

Milestone 1 itself (GPIO end-to-end) is done offline; criterion 4 (LEDs on a
real board) is the first item of the hardware checklist.

## Current state and what is left

All work that can be checked without a board is done for the scope above, and
CI runs the runtime tests as 32-bit ARM binaries under `qemu-arm`. What
remains:

- **Needs the board (blocking):** the checklist in
  [08](08-verification-gaps.md), starting with `board_check`.
- **Offline, not started:**
  - Zynq UltraScale+ (different clock registers, interrupt numbering, bitstream
    handling and a 64-bit target); can be type-checked and emulated, not proven.
  - HLS AXI-Stream and array (BRAM) port arguments; AXI DMA scatter-gather.
  - Multiple address blocks per instance (08, D3); UIO maps beyond map 0 (D7).
  - Publishing the runtime crates (so generated crates need no
    `--hardroxide-path`), and a `cargo hardroxide` shim (D6).
  - HWH samples from other Vivado versions (needs those tools; D5).

## Documentation site

The mdBook (`book.toml`, `src = "docs"`) is the authoritative documentation
and is deployed to GitHub Pages by `.github/workflows/pages.yml` together with
`cargo doc` under `api/`. Guard tests keep it honest:
`crates/hardroxide-cli/tests/book.rs` checks reference signatures against the
source, CLI help blocks against the real output, and relative links;
`crates/hardroxide-gen/tests/guide.rs` compiles every guide example tagged
`rust,ignore,fixture=<name>` against a freshly generated crate (`# ` lines are
hidden setup). Keep the reference in step with every API change.
