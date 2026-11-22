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
- Runtime: Linux userspace on the PS (PL only; Linux owns PS peripherals), or, since 2.0, bare metal or the `hardroxide-rt` kernel on core 0.
- Generated crate embeds the bitstream (raw `.bin`) and keeps an interface hash
  and a bitstream hash so `hardroxide update` can swap the bitstream without
  regenerating code when the interface is unchanged.
- MMIO is a pluggable backend (`/dev/mem`, UIO, mock). DMA defaults to uncached
  contiguous buffers behind a `DmaBuffer` trait.
- Sync core with a feature-gated async API. Typed errors via `thiserror`.
- Rust latest stable, edition 2024. License `MIT OR Apache-2.0`.
- Out of scope (1.0 and 2.0): partial reconfiguration, Python bindings.

## Platforms (2.0)

`hardroxide generate --platform linux|baremetal|rtos` ([ADR 0003](docs/decisions/0003-bare-metal-and-rtos-platforms.md)).
Bare-metal/RTOS crates run on core 0 with no OS (`crates/hardroxide-ps7`, `crates/hardroxide-rt`, no third-party
dependencies); `hardroxide run`/`package`/`doctor` drive the board with the Vitis tools (JTAG boot mode for `run`).
Their tests: `scripts/test-qemu.sh`, `cargo test -p hardroxide-gen --test baremetal` and `--test apps`, the board
regression `scripts/test-board.sh`, programs in `baremetal/` (own workspace) and applications in `apps/` (built by the
`apps` test against the generated crate; plan in ADR 0004, versions 2.1 to 2.7). `main` carries 2.7.0, released.

## Current state and next step

Zynq-7000 only. `hardroxide generate`, `update` and `inspect` work (golden
snapshots, mock-MMIO tests, compile-tested guide examples, armv7 emulation tests)
and the drivers were **validated on a PYNQ-Z2 on 2026-11-16**; results and the
bugs that validation found are in `docs/08-verification-gaps.md`. Crates: hwh,
mmio, fpga, dma, irq, ip (GPIO, HLS, DMA, Timer, IIC, SPI, INTC, BRAM, UART,
FIFO, audio, VDMA, VTC, dynclk), gen, cli; for bare metal and the RTOS: ps7, rt; the applications on the PYNQ base
overlay (ADR 0004): video, gfx, audio, fat, net, crypto, tls (`docs/reference/libraries.md` says what each can and cannot do). On-board test programs are in `fixtures/board_tests/`; real
third-party designs (PYNQ overlays) are in `fixtures/real/`.

**The mdBook (`docs/`, deployed by `.github/workflows/pages.yml`) is the
authoritative documentation**, including twenty-one tutorials (`docs/tutorials/`, grouped by flavor: `linux/`, `baremetal/`, `rtos/`, and `apps/` for applications) whose code lives in `fixtures/board_tests/` (Linux) and `fixtures/board_tests_bm/` (bare metal, RTOS; compiled by `cargo test -p hardroxide-gen --test board_tests` and `--test baremetal`) and whose outputs were captured on a board; board helper scripts are in `scripts/board/`. When you change a public API or the CLI, update
`docs/reference/` in the same change; `cargo test -p hardroxide-cli --test book`
fails if signatures, help text or links drift. Guide examples are compile-tested
(`cargo test -p hardroxide-gen --test guide`; tag blocks
`rust,ignore,fixture=<name>`, `# ` lines are hidden setup).

**Next step:** 2.7.0 is released (tag `v2.7.0` and a GitHub release; 2.0.0 and 2.1.0 are tagged too; nothing is on crates.io, publishing needs the maintainer's token, `docs/release.md`). ADR 0004's plan is complete. Open items, recorded in `docs/05-roadmap.md` ("After 2.7"): a hardware random source in the PL (the TLS
random bytes are weak), faster public-key arithmetic or PL accelerators (a TLS handshake is about 80 ms of the core), HDMI input capture, regenerating the other board fixtures' `ps7_init`, `fsck.fat` on a board-formatted card, listening to the audio, an interactive browser session, a longer soak, an independent review of the crypto and TLS.

On a board: stop anything using the PL before the first `load()` (it refuses if a
driver is bound to a PL address; reprogramming under one froze the CPU), and
reboot to switch between designs whose overlays need a different trigger type on
the same interrupt line.

## Conventions

- Generated driver constructors may only use values from
  `hardroxide_hwh::Instance::driver_config()` (it is hashed into the interface
  hash; anything else would go stale on a bitstream-only `hardroxide update`).
- The runtime crates (`mmio`, `fpga`, `dma`, `irq`, `ip`, and the application libraries `gfx`, `video`, `audio`, `fat`, `net`, `crypto`, `tls`) must stay `no_std`-capable: new code that needs the OS goes behind the `std` feature, and `scripts/check-no-std.sh` (CI) builds them for `armv7a-none-eabihf`. Timeouts use `hardroxide_mmio::time`, never `std::time`.
- `scripts/test-armv7.sh` runs the runtime crates' tests as armv7 binaries
  under `qemu-arm` (uses the Vitis-bundled gcc/qemu if no system ones; CI too).

- Cargo workspace monorepo, crates prefixed `hardroxide-*`.
- Generation must be deterministic. Minimize and document `unsafe`.
- Tests: golden XSA fixtures with snapshot tests, plus mock-MMIO unit tests.
  Hardware tests are manual on the board.
