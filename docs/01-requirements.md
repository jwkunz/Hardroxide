# 01 - Requirements

Status: draft, updated through interview round 2.

## Decisions from interview (round 2)

| Topic | Decision |
|---|---|
| Generator delivery | `hardroxide` CLI (also invocable as `cargo hardroxide`) with `generate` as the core command, plus build, deploy, run helpers. The generator logic lives in a library crate so other front ends can reuse it. |
| Interrupts / concurrency | Sync (blocking, thread-based) core with an optional async feature, both designed in from day one. |
| License | `MIT OR Apache-2.0` |

## MVP curated IP set

All four categories are in scope for the first milestone:

1. AXI GPIO
2. AXI DMA (direct register mode first)
3. AXI Timer, AXI IIC, AXI SPI (with `embedded-hal` trait impls where they fit)
4. HLS / custom IP via HWH register maps (`ap_ctrl_hs` / `ap_ctrl_chain`)

Note: this is a broad MVP. Suggested internal ordering: GPIO, then HLS/custom
(proves the HWH-driven path), then DMA, then Timer/IIC/SPI.

## Functional requirements (draft)

- FR-1: Accept an `.xsa` and produce a Cargo crate containing the embedded
  bitstream, generated register layer, and a typed top-level design struct.
- FR-2: Generation is deterministic: identical XSA yields byte-identical output.
- FR-3: `load()` programs the PL, configures PL clocks/resets, and maps all IP.
- FR-4: Each IP instance in the HWH appears as a typed field; unknown IP exposes
  raw register access.
- FR-5: Embed a hash of bitstream + HWH; mismatch is detected at load time.
- FR-6: Blocking and (feature-gated) async interrupt waiting.
- FR-7: Safe DMA buffer type that tracks hardware ownership.

## Non-functional requirements (draft)

- NFR-1: Generated crate builds with stable Rust.
- NFR-2: Generator does not require Vivado to be installed.
- NFR-3: Minimize and isolate `unsafe`; document each use.
- NFR-4: Board- and device-specific data is isolated behind a board/device profile.

## Open questions

See `04-runtime.md` (round 3) for runtime mechanism choices.
