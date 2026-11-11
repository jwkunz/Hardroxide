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
3. HLS / custom IP generation from HWH register maps.
4. AXI DMA with uncached `DmaBuffer`, UIO interrupts (sync, then async feature).
5. AXI Timer, IIC, SPI with `embedded-hal` trait impls.
6. Cached DMA backend, then partial reconfiguration and UltraScale+ support.

## Open items

- Obtain real XSA fixtures (GPIO-only first) from Vivado 2025.2.
- Inspect what register data a real HWH exposes before finalizing the model.
