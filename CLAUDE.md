# Hardroxide

Rust code generator and runtime that turns a Vivado XSA (bitstream + hardware
handoff) into a typed Rust HAL crate: a Rust counterpart to the PYNQ overlay
framework. Tool: `hardroxide generate <design.xsa>`.

## Start here

Design decisions from the spec interview live in `docs/` (read `docs/README.md`
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

Milestone 1 in progress, hardware-free portion. `fixtures/pynq_z2_gpio.xsa`
exists (built with Vivado 2025.2 at `~/AMD/2025.2`; see
`docs/07-hwh-findings.md`) and `hardroxide-hwh` parses it. No board is attached
to the dev machine; hardware validation (fpga_manager `.bin` format, /dev/mem
mapping, LED blink) remains manual.

## Conventions

- Cargo workspace monorepo, crates prefixed `hardroxide-*`.
- Generation must be deterministic. Minimize and document `unsafe`.
- Tests: golden XSA fixtures with snapshot tests, plus mock-MMIO unit tests.
  Hardware tests are manual on the board.
