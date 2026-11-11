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

Specs only; no code yet. Milestone 1 is GPIO end-to-end on the PYNQ-Z2
(see `docs/05-roadmap.md`). The immediate next step, on the Vivado machine, is
to build a GPIO-only design for the PYNQ-Z2 in Vivado 2025.2, export the XSA to
`fixtures/`, and inspect its HWH (which register data it actually exposes)
before finalizing the data model in `hardroxide-hwh`.

## Conventions

- Cargo workspace monorepo, crates prefixed `hardroxide-*`.
- Generation must be deterministic. Minimize and document `unsafe`.
- Tests: golden XSA fixtures with snapshot tests, plus mock-MMIO unit tests.
  Hardware tests are manual on the board.
