# Designs for running on a board

`pynq_z2_*.xsa` are Hardroxide's own test designs for the PYNQ-Z2. The `*base_overlay*.xsa` files are the boards' PYNQ
base overlays, which the examples use:

| File | From | License |
|---|---|---|
| `pynq_base_overlay.xsa` | the PYNQ project's base overlay, with a `ps7_init` | Apache-2.0 (https://github.com/Xilinx/PYNQ) |
| `rfsoc4x2_base_overlay.xsa` | RFSoC-PYNQ 4.0.0's base overlay (`base.bit`, `base.hwh` unchanged), with a `psu_init` | BSD 3-Clause, `LICENSE.rfsoc-pynq` (https://github.com/Xilinx/RFSoC-PYNQ) |

How each is made: `docs/reference/base-overlay.md` and `docs/reference/fixtures.md`.
