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
