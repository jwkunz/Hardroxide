# RFSoC 4x2 fixtures

Designs for the Real Digital RFSoC 4x2 (`xczu48dr-ffvg1517-2-e`), Vivado 2025.2, built for [ADR 0005](../../docs/decisions/0005-ultrascale-plus.md). Each XSA holds the bitstream, the hardware handoff and the board's `psu_init`.

| XSA | What is in it | Control, interrupts |
|---|---|---|
| `rfsoc4x2_gpio` | LEDs, RGB LEDs, buttons, switches on AXI GPIO (the only design with pins); also the processor-only design | HPM0_FPD; irq0[0] |
| `rfsoc4x2_intc` | AXI Timer, IIC, two GPIOs behind an AXI INTC | HPM0_FPD; irq0[0] |
| `rfsoc4x2_loopback` | timer, 2 UART Lite, 2 IIC, Quad SPI, 2 GPIO, stream FIFO, 64 KiB block RAM, wired back to each other inside the FPGA | HPM0_LPD; irq0[7:0] |
| `rfsoc4x2_dma` | two AXI DMA stream loops: 32-bit addresses and 32-bit data on HP0; 40-bit addresses and 128-bit data on HP1 | HPM0_FPD; irq0[1:0], irq1[1:0] |
| `rfsoc4x2_hls_vadd` | HLS kernel, AXI-Lite only | HPM0_FPD; irq0[0] |
| `rfsoc4x2_hls_scale` | HLS kernel with two memory masters (64-bit pointers) on HP0 | HPM0_FPD; irq0[0] |
| `rfsoc4x2_rf_loopback` | RF data converter: 2 DACs fed by waveform players, 2 ADCs captured by DMA (see `scripts/build_rf_loopback.tcl`) | HPM0_FPD; irq0[1:0] |

## Building

`scripts/build.sh <name>...` (names without the `rfsoc4x2_` prefix) writes the XSA here; it fails if timing is not met. It needs a Vivado license for the part. `scripts/sim_rf.sh` simulates the RF design's own logic (`hdl/`) and needs none.

## Probing on a board

`scripts/probe.py <name>...` programs the board over JTAG (JTAG boot mode; volatile) and exercises the design with raw register accesses from `xsdb`, using no Hardroxide code: so a later failure of a driver can be told from a fault of the design. The `gpio` probe also tests DDR in both address ranges.

For the RF loopback, connect SMA cables from DAC A to ADC A and from DAC B to ADC B, and set `HRX_RF_LOOPBACK=1` to require both tones.

| Board connector | In the design |
|---|---|
| DAC A | DAC tile 2, block 0: `vout20`, player 1 |
| DAC B | DAC tile 0, block 0: `vout00`, player 0 |
| ADC A | ADC tile 2, `vin2_23`: capture input 1 |
| ADC B | ADC tile 2, `vin2_01`: capture input 0 |

The DACs were identified by unplugging DAC A's cable; the ADCs follow from the cabling above.

## Third-party files

`board_files/` (Real Digital, Apache 2.0) and `clocks/` (Xilinx RFSoC-PYNQ, BSD 3-Clause); each has its license and origin.
