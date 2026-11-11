# Hardroxide

Rust code generator and runtime that turns a Vivado XSA (bitstream + hardware
handoff) into a typed Rust HAL crate: a Rust counterpart to the PYNQ overlay
framework. Tool: `hardroxide generate <design.xsa>`.

## Start here

Design decisions from the spec interview live in `docs/` (read `docs/design-index.md`
first, then `docs/06-session-handoff.md` for background, risks, and the first
steps; ADRs with the tradeoff reasoning are in `docs/decisions/`). Treat them as the source of truth; update the relevant doc when a
decision changes.

## Key decisions (summary)

- First board: PYNQ-Z2 (XC7Z020, armv7), Vivado 2025.2, stock PYNQ Linux image.
- Runtime: Linux userspace on the PS. PL only; Linux owns PS peripherals.
- Generated crate embeds the bitstream (raw `.bin`) and keeps an interface hash
  and a bitstream hash so `hardroxide update` can swap the bitstream without
  regenerating code when the interface is unchanged.
- MMIO is a pluggable backend (`/dev/mem`, UIO, mock). DMA defaults to uncached
  contiguous buffers behind a `DmaBuffer` trait.
- Sync core with a feature-gated async API. Typed errors via `thiserror`.
- Rust latest stable, edition 2024. License `MIT OR Apache-2.0`.
- Out of scope for v1: partial reconfiguration, Python bindings.

## Current state and next step

Milestone 1 hardware-free portion is done: `hardroxide generate` produces a
crate from `fixtures/pynq_z2_gpio.xsa` (Vivado 2025.2 at `~/AMD/2025.2`), with
golden snapshots and mock-MMIO tests passing (`cargo test --workspace`). Crates:
hwh, mmio, fpga, dma, irq, ip (GPIO, HLS, DMA, Timer, IIC, SPI, INTC, BRAM, UART), gen, cli. No board is attached to the dev machine.

**Next step: run the checklist in `docs/08-verification-gaps.md` on the
PYNQ-Z2**, in order (G1 bitstream load first; do not touch PL addresses before
it passes). Milestone 2 (`hardroxide update`, `dev-bitstream`) is done
offline. Milestone 3 (scalar HLS IP, `fixtures/pynq_z2_hls_vadd.xsa`) is done offline.
Milestone 4 (AXI DMA direct mode, `hardroxide-dma`, `hardroxide-irq`, DMA loopback
fixture) is done offline. Milestone 5 (Timer/IIC/SPI with embedded-hal, `fixtures/pynq_z2_peripherals.xsa`)
is done offline. **Docs site: an mdBook (`book.toml`, source = `docs/`) is the authoritative
API documentation, deployed to GitHub Pages by `.github/workflows/pages.yml`.
When you change a public API or the CLI, update `docs/reference/` in the same
change; `cargo test -p hardroxide-cli --test book` fails if signatures, help
text or links drift. Logo: `docs/assets/logo.png`.** Interrupts are wired into generated crates (`IrqLine`, `interrupts.dtso`).
Guide examples are compile-tested (`cargo test -p hardroxide-gen --test guide`; tag blocks `rust,ignore,fixture=<name>`, `# ` lines are hidden setup). AXI interrupt controller fan-in is supported (`AxiIntc`, `via_intc`, `fixtures/pynq_z2_intc.xsa`). HLS pointer (`m_axi`) arguments get `set_*_addr`/`set_*_buffer` helpers (`fixtures/pynq_z2_hls_scale.xsa`). Generated crates include a `board_check` example (load, FCLK readback, UIO devices) to run first on a board. AXI BRAM Controller and UART Lite drivers exist (`fixtures/pynq_z2_mem_uart.xsa`). Remaining offline work: more IP drivers (stream FIFO, others), AXI-Stream HLS ports, more fixtures.

## Conventions

- Cargo workspace monorepo, crates prefixed `hardroxide-*`.
- Generation must be deterministic. Minimize and document `unsafe`.
- Tests: golden XSA fixtures with snapshot tests, plus mock-MMIO unit tests.
  Hardware tests are manual on the board.
