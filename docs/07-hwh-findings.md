# 07 - HWH findings (Vivado 2025.2)

Source: `fixtures/pynq_z2_gpio.xsa`, built by
`fixtures/scripts/build_pynq_z2_gpio.tcl` (PS7 + AXI GPIO, dual channel:
4 LEDs out, 4 buttons + 2 switches in; no PYNQ-Z2 board file, PS7 configured
by hand, so DDR/MIO are not board-accurate; the PL side is).

Rebuild: `source ~/AMD/2025.2/Vivado/settings64.sh && vivado -mode batch -source
fixtures/scripts/build_pynq_z2_gpio.tcl -tclargs <workdir> fixtures/pynq_z2_gpio.xsa`.

## XSA contents

Flat zip: `design_1.hwh`, `<name>.bit`, `ps7_init.{c,h,tcl,html}`, `sysdef.xml`,
`xsa.json`, `xsa.xml`, `design_1.bda`. The `.hwh` starts with a UTF-8 BOM.

## What the HWH exposes

- **Register maps for Xilinx catalog IP are present** (resolves the biggest
  risk in 06): `ADDRESSBLOCKS/ADDRESSBLOCK/REGISTERS/REGISTER` with
  `ADDRESS_OFFSET`, `ACCESS`, `RESET_VALUE`, and `FIELDS` with `BIT_OFFSET` /
  `BIT_WIDTH`. A curated `hardroxide-ip` library is still wanted for
  ergonomic APIs, but the PAC layer can be generated purely from the HWH.
- Quirk: `REGISTER SIZE` is inconsistent (AXI GPIO data regs report `4`, the
  field width; GIER reports `32`). Treated as a hint only.
- Quirk: `IP_IER`/`IP_ISR` offsets are not in ascending order in the file.
- Address map: `MODULE[PS]/MEMORYMAP/MEMRANGE` (`INSTANCE`, `BASEVALUE`,
  `HIGHVALUE`, inclusive high). The instance also has `C_BASEADDR` params.
- PL clocks: PS7 parameters `PCW_ACT_FPGAn_PERIPHERAL_FREQMHZ`,
  `PCW_EN_CLKn_PORT`, `PCW_EN_RSTn_PORT`.
- PS modules have `IS_PL="FALSE"`.
- Interrupts: parsed from output ports with `SIGIS="INTERRUPT"`; sink routing
  (to PS7 `IRQ_F2P`) is not yet resolved (no IRQ in this fixture).

## Implemented

`hardroxide-hwh`: `Xsa::open`, `parse_hwh`, `BitFile` (header strip and
byte-swapped `.bin`). The byte-swap convention is **unverified on hardware**.

## HLS fixture (`fixtures/pynq_z2_hls_vadd.xsa`)

Built from `fixtures/hls/vadd.cpp` (`out = a + b`, all `s_axilite`, `ap_ctrl_hs`)
with `vitis-run --mode hls --package --config fixtures/hls/hls.cfg`, then
`fixtures/scripts/build_pynq_z2_hls.tcl` (PS7 + vadd_0, `interrupt` on
`IRQ_F2P`). Findings:

- VLNV `xilinx.com:hls:vadd:1.0`; the library field `hls` identifies HLS IP.
- The HWH carries the full register map: `CTRL` (0x00: AP_START, AP_DONE,
  AP_IDLE, AP_READY, AUTO_RESTART, INTERRUPT), `GIER` 0x04, `IP_IER` 0x08,
  `IP_ISR` 0x0C, then user registers (`a` 0x10 write-only, `b` 0x18,
  `out_r` 0x20 read-only, `out_r_ctrl` 0x24 with `out_r_ap_vld`).
  Register `ACCESS` is accurate here (write-only / read-only per register).
- User register names keep the C argument names (lower-case); a returned
  pointer argument `out` is renamed `out_r` by HLS.
- Interrupts: the HLS module has `PORT DIR="O" SIGIS="INTERRUPT"` with a
  `CONNECTIONS/CONNECTION INSTANCE="processing_system7_0" PORT="IRQ_F2P"`.
  The PS7 `IRQ_F2P` port lists the reverse connection. With a single source
  the vector is 1 bit wide; sources behind `xlconcat` are not resolved yet.
- AXI GPIO's `ip2intc_irpt` also appears as an (unconnected) interrupt port.

## DMA loopback fixture (`fixtures/pynq_z2_dma_loopback.xsa`)

`fixtures/scripts/build_pynq_z2_dma.tcl`: PS7 (HP0 enabled) + `axi_dma:7.1`
(direct register mode, `c_include_sg=0`, length width 23) with MM2S looped to
S2MM through an `axis_data_fifo`; both interrupts via `xlconcat` to `IRQ_F2P`.

- Full register maps are present (MM2S_/S2MM_ DMACR, DMASR, SA/DA, LENGTH,
  SG registers even in simple mode, with fields). `hardroxide-ip`'s offsets and
  bit positions are asserted against this HWH in a unit test.
- `C_SG_LENGTH_WIDTH` (parameter) bounds the transfer length (`2^w - 1`); the
  generator passes it to `AxiDma::with_length_width`.
- Interrupt nets: DMA `mm2s_introut` -> `xlconcat_0/In0`, `s2mm_introut` ->
  `In1`, `xlconcat_0/dout` -> PS7 `IRQ_F2P[1:0]`. `Design::ps7_irq_f2p_bit`
  resolves these by following `xlconcat` (`INk_WIDTH` parameters) to bit 0 / 1.
- The HP0 data path and the loopback FIFO do not appear in the address map
  (only the control interface at 0x40400000-0x4040FFFF).
