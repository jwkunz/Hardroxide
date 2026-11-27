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
  sections, tickless idle.
- Both cores (SMP), UltraScale+, and more bare-metal tutorials (timers and interrupts, DMA).

## 2.1 to 2.7: applications on the base overlay

Accepted in [ADR 0004](decisions/0004-base-overlay-applications.md): the PYNQ base overlay brought up bare metal and RTOS (2.1), a
graphics library on the HDMI output (2.2), WAV playback and recording (2.3), an SD card and FAT32 (2.4, board-verified, merged), Ethernet and a web
server (2.5, board-verified, merged), HTTPS with a self-signed certificate (2.6, board-verified), and an integrated application (2.7, board-verified), one minor version each, each with board-verified acceptance tests. Later: PL accelerators for the TLS hot spots and a hardware random number generator (see the ADR).

## After 2.7 (released)

Not scheduled, recorded so they are not forgotten: a **hardware random source** in the PL (a ring-oscillator or the XADC noise) for TLS, whose random bytes are weak today ([TLS reference](reference/tls.md#where-the-random-bytes-come-from)); **faster
public-key arithmetic** or PL accelerators for the TLS handshake (about 80 ms of the single core); **HDMI input capture** (the VDMA write channel and the overlay's HDMI receiver); regenerating the **other board fixtures' `ps7_init`** with the Ethernet and 1.8 V
bank fixes; `fsck.fat` on a card the board formatted; listening to the audio pump; an interactive browser session and Firefox and Safari against the HTTPS server; an independent review of the cryptography and the TLS code; and the older items under "Not started" above
(UltraScale+, SMP, HLS stream ports, scatter-gather DMA).

## 3.0: Zynq UltraScale+ (in progress, branch `v3`)

Plan and results: [ADR 0005](decisions/0005-ultrascale-plus.md). Done: the FPGA fixture designs, processor support,
`hardroxide run`, the kernel, SD, Ethernet, I2C, the board's own hardware, and code generation (bare metal and RTOS)
with the drivers, the base overlay applications on generated crates, and a two-hour timing run of the kernel. Linux
crates run on the board, DMA included, with tutorials (the FPGA's DDR4 among them); the kernel serves a web page beside a
control loop. `hardroxide doctor` and `package` handle the family (an image booted from an SD card on the board). To do: the
radio driver (3.1), then the release, which carries both. Left out: USB, Pmod+ and
SYZYGY, and DisplayPort and the 100 Gb port (nothing to test them with). The PYNQ-Z2
board regression ran on 2026-11-26 and found a register-saving bug of the 2.x releases, now fixed here; its SD, audio
loopback and Linux checks are still owed.

## Documentation site

The mdBook (`book.toml`, `src = "docs"`) is the authoritative documentation
and is deployed to GitHub Pages by `.github/workflows/pages.yml` (via `scripts/build-site.sh`) with a landing page, the book under `/book/` and
`cargo doc` under `/api/`. Guard tests keep it honest:
`crates/hardroxide-cli/tests/book.rs` checks reference signatures against the
source, CLI help blocks against the real output, and relative links;
`crates/hardroxide-gen/tests/guide.rs` compiles every guide example tagged
`rust,ignore,fixture=<name>` against a freshly generated crate (`# ` lines are
hidden setup). Keep the reference in step with every API change.
