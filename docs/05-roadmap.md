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

The scope above is done and was validated on a PYNQ-Z2 on 2026-11-16 (results
in [08](08-verification-gaps.md)); CI runs the runtime tests as 32-bit ARM
binaries under `qemu-arm`. The first release is prepared (see
[Releasing](release.md)). What remains:

- **Not started:**
  - Zynq UltraScale+ (different clock registers, interrupt numbering, bitstream
    handling and a 64-bit target); `generate` refuses these designs today.
  - HLS AXI-Stream and array (BRAM) port arguments; AXI DMA scatter-gather.
  - Multiple address blocks per instance (08, D3); UIO maps beyond map 0 (D7).
  - A `cargo hardroxide` shim (D6).
- **Needs more inputs:** HWH from Vivado 2021 to 2023 and more third-party
  designs (D5); SPI against a real device; an automated on-board regression
  script around `fixtures/board_tests/`.

## 2.0: bare metal and RTOS

Done and verified on a PYNQ-Z2 and under QEMU ([ADR 0003](decisions/0003-bare-metal-and-rtos-platforms.md), results in
[08](08-verification-gaps.md)): `--platform baremetal|rtos`, `hardroxide-ps7` (startup, GIC, timers, UART, MMU, caches),
`hardroxide-rt` (tasks, semaphores, mutexes with priority inheritance, queues), `hardroxide run`/`package`/`doctor`.
What remains:

- Booting a `hardroxide package` image from SD, cached DMA buffers with explicit cache maintenance against a PL master.
- Kernel features listed under [Not yet](reference/rt.md#not-yet): event flags, software timers, GIC-priority-mask critical
  sections, tickless idle, a blocking `IrqWait` integration.
- Both cores (SMP), UltraScale+, and more bare-metal tutorials (timers and interrupts, DMA).

## 2.1 to 2.6: applications on the base overlay

Proposed in [ADR 0004](decisions/0004-base-overlay-applications.md): the PYNQ base overlay brought up bare metal and RTOS (2.1), a
graphics library on the HDMI output (2.2), WAV playback and recording (2.3), an SD card and FAT32 (2.4), Ethernet and a web
server (2.5), and an integrated application (2.6), one minor version each, each with board-verified acceptance tests.

## Documentation site

The mdBook (`book.toml`, `src = "docs"`) is the authoritative documentation
and is deployed to GitHub Pages by `.github/workflows/pages.yml` (via `scripts/build-site.sh`) with a landing page, the book under `/book/` and
`cargo doc` under `/api/`. Guard tests keep it honest:
`crates/hardroxide-cli/tests/book.rs` checks reference signatures against the
source, CLI help blocks against the real output, and relative links;
`crates/hardroxide-gen/tests/guide.rs` compiles every guide example tagged
`rust,ignore,fixture=<name>` against a freshly generated crate (`# ` lines are
hidden setup). Keep the reference in step with every API change.
