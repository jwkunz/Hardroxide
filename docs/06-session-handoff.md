# 06 - Session Handoff Notes

Context from the original design session (machine without Vivado), written so a
new Claude session on the Vivado machine can resume without the chat history.

## Origin of the idea

The user wanted a PYNQ-like overlay system for AMD hardware but in Rust instead
of Python. The first idea was a runtime library with a build-time macro. The
user then proposed a better one: a binary that takes the bitstream and hardware
handoff and writes a HAL crate (analogous to `svd2rust` for PL). That became
Hardroxide. Input was refined from loose `.bit` + `.hwh` to the `.xsa`, which
bundles both. The name was chosen by the user; the tool is `hardroxide`, with
`hardroxide generate` creating the crate.

## How PYNQ works (what we are replicating)

1. Overlay = `.bit` + `.hwh` (Vivado hardware handoff XML: IP instances, VLNV,
   address map, interrupts, clocks, resets).
2. Bitstream loaded via the Linux FPGA manager (`/sys/class/fpga_manager`, or
   `/dev/xdevcfg` on old Zynq-7000 kernels).
3. Registers mapped via `/dev/mem` or UIO.
4. Drivers bound to IP by VLNV, with a generic fallback.
5. Contiguous DMA buffers via CMA / udmabuf / dma-heap.
6. PL clocks and resets via PS clock controller. Interrupts via UIO.

Why Rust helps: compile-time typed access to every IP instance, typed
bitfields, DMA buffers whose ownership the compiler tracks, a single static
binary with no Python runtime on the board.

## Architecture in one picture

```
design.xsa --> hardroxide generate --> my_design/ (Cargo crate)
                                         |- bitstream/design.bin (embedded)
                                         |- src/pac.rs   (generated registers)
                                         `- src/lib.rs   (Design, load())
Layers: PAC (generated) -> curated IP drivers by VLNV (hardroxide-ip,
embedded-hal impls where they fit) -> generated Design struct.
Unknown IP falls back to raw register access.
```

## Full list of interview decisions (with where each is recorded)

- Runtime on Linux/PS, bare-metal maybe later (00-vision, 04-runtime)
- UX: library-first plus thin REPL/CLI, equal priority (00-vision)
- Register source: HWH where present + curated IP library (00-vision)
- Scope: open source, general AMD (Zynq-7000 and UltraScale+) (00-vision)
- CLI: `hardroxide`, also `cargo hardroxide` (01, 02)
- IRQ: sync core plus feature-gated async, both from day one (01)
- MVP IP: GPIO, DMA, Timer/IIC/SPI, HLS/custom; suggested order GPIO, HLS,
  DMA, Timer/IIC/SPI (01)
- License MIT OR Apache-2.0 (01)
- Stock PYNQ image first (04)
- MMIO pluggable; DMA uncached default (04, ADR 0001)
- Testing: golden XSA fixtures plus mock MMIO; hardware tests manual (04)
- No partial reconfiguration or Python in v1; clocks from HWH (04)
- Monorepo workspace; single generated crate depending on runtime crates;
  PL only (02)
- Bitstream embedded `.bin`, two hashes, `hardroxide update` (03, ADR 0002)
- Singleton `Design`; snake_case instance names; `thiserror` errors (03)
- Rust latest stable, edition 2024; HLS typed registers plus ap_ctrl helpers;
  Milestone 1 is GPIO end-to-end (05)

## Risks and unknowns to resolve first

- **HWH content is the biggest unknown.** Newer Vivado HWH files often include
  register maps for HLS/user IP, but this must be verified against real
  Vivado 2025.2 output before locking `hardroxide-hwh`'s model. HWH is loosely
  specified and varies by Vivado version; collect several samples.
- Xilinx catalog IP (AXI GPIO etc.) is not described register-by-register in
  the HWH, hence the curated `hardroxide-ip` library keyed by VLNV.
- Custom RTL IP without register description gets base address plus raw
  read/write unless a sidecar (TOML/SVD) is supplied.
- Stock PYNQ image feature check: fpga_manager flags, UIO device-tree overlay
  for interrupts, udmabuf availability. Verify on the real board.
- Cache maintenance differs ARMv7 vs ARMv8, relevant when adding cached DMA and
  UltraScale+ support.

## First steps on the Vivado machine

1. Clone the repo; read `CLAUDE.md` and `docs/`.
2. Build a minimal PYNQ-Z2 design in Vivado 2025.2: Zynq PS + AXI GPIO wired
   to the board LEDs (and buttons). Export the XSA.
3. Commit it as `fixtures/pynq_z2_gpio.xsa` (small; keep Vivado project
   directories out of git).
4. Unzip it and inspect the `.hwh`: IP list, address map, FCLK settings,
   interrupts, any register data. Record findings in `docs/` and adjust the
   data model.
5. Scaffold the Cargo workspace (`hardroxide-hwh` first) and parse that XSA.
6. Optionally add an HLS design and a DMA loopback design as further fixtures.

## Working preferences observed

- Prefers recommended defaults, but wants tradeoffs explained when a choice
  involves performance vs security vs portability.
- Wants specs recorded in `docs/` as decisions are made.
- Questions with several options at once were sometimes too much; ask fewer,
  more focused questions.
- Git remote: `git@github.com:jwkunz/Hardroxide.git` (SSH), branch `main`.
