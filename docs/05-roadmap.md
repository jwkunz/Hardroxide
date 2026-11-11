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

## Suggested later milestones

2. `hardroxide update` and interface/bitstream hashing, `dev-bitstream` loader. **Done offline** (see 08, D4); hardware unverified.
3. HLS / custom IP generation from HWH register maps. **Done offline** for scalar AXI-Lite HLS IP (vadd fixture); hardware unverified (08, G9).
4. AXI DMA with uncached `DmaBuffer`, UIO interrupts (sync, then async feature). **Done offline** (08, G10/G11); not yet integrated into generated `Design` for IRQs.
5. AXI Timer, IIC, SPI with `embedded-hal` trait impls. **Done offline** (08, G12): `DelayNs`, `I2c`, `SpiBus`/`SpiDevice`.
6. Cached DMA backend, then partial reconfiguration and UltraScale+ support.

## Open items

- Obtain real XSA fixtures (GPIO-only first) from Vivado 2025.2.
- Inspect what register data a real HWH exposes before finalizing the model.

## Documentation site (requested, not started)

Publish a GitHub Pages site built around an **mdBook** that is the
authoritative API documentation for Hardroxide: generator CLI, generated-crate
layout, each runtime crate (`hardroxide-mmio`, `-fpga`, `-dma`, `-irq`, `-ip`,
`-hwh`, `-gen`), the register/PAC conventions, and the verification-gap
checklist. Requirements to settle when starting:

- Book lives in-repo (e.g. `book/`), built and deployed by a GitHub Actions
  Pages workflow on `main`.
- "Authoritative API docs": decide how it relates to rustdoc (link/embed
  `cargo doc` output under the site, and/or keep the book as the normative
  reference with examples that are compile-tested, e.g. via `mdbook test`).
- Absorb `docs/` (specs, ADRs, HWH findings, verification gaps) into the book
  rather than keeping two copies.
- Use `docs/assets/logo.png` (see README/branding below).
