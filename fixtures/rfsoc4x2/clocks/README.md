# RFSoC 4x2 clock chip register files

`LMK04828_245.76.txt` and `LMX2594_491.52.txt`, unchanged from [RFSoC-PYNQ](https://github.com/Xilinx/RFSoC-PYNQ) commit `2a6beba`
(`boards/RFSoC4x2/edf_bsp/meta-remote/recipes-apps/xrfclk-tics/files/`), under the BSD 3-Clause license in `LICENSE`.

Each line is one 24-bit SPI write, in order. The chips are on the processor's SPI 0: chip select 0 is the LMK04828, 1 and 2 the
LMX2594s of the DACs and the ADCs. They make the converters' 491.52 MHz reference clocks; see `scripts/probe_rf_loopback.tcl`
for the sequence.
