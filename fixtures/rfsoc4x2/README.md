# RFSoC 4x2 fixtures

**These are driver tests, not examples.** Examples and tutorials use the board's base overlay,
`../board/rfsoc4x2_base_overlay.xsa` (probed with `scripts/probe.py base`).

Designs for the Real Digital RFSoC 4x2 (`xczu48dr-ffvg1517-2-e`), Vivado 2025.2, built for [ADR 0005](../../docs/decisions/0005-ultrascale-plus.md). Each XSA holds the bitstream, the hardware handoff and the board's `psu_init`.

| XSA | What is in it | Control, interrupts |
|---|---|---|
| `rfsoc4x2_gpio` | LEDs, RGB LEDs, buttons, switches on AXI GPIO (the only design with pins; button bit 4 is the user reset button, which reads high until pressed); also the processor-only design | HPM0_FPD; irq0[0] |
| `rfsoc4x2_intc` | AXI Timer, IIC, two GPIOs behind an AXI INTC | HPM0_FPD; irq0[0] |
| `rfsoc4x2_loopback` | timer, 2 UART Lite, 2 IIC, Quad SPI, 2 GPIO, stream FIFO, 64 KiB block RAM, wired back to each other inside the FPGA | HPM0_LPD; irq0[7:0] |
| `rfsoc4x2_dma` | two AXI DMA stream loops: 32-bit addresses and 32-bit data on HP0; 40-bit addresses and 128-bit data on HP1 | HPM0_FPD; irq0[1:0], irq1[1:0] |
| `rfsoc4x2_hls_vadd` | HLS kernel, AXI-Lite only | HPM0_FPD; irq0[0] |
| `rfsoc4x2_hls_scale` | HLS kernel with two memory masters (64-bit pointers) on HP0 | HPM0_FPD; irq0[0] |
| `rfsoc4x2_rf_loopback` | RF data converter: 2 DACs fed by waveform players, 2 ADCs recorded together by one DMA (see `scripts/build_rf_loopback.tcl`) | HPM0_FPD; irq0[1:0] |

## Building

`scripts/build.sh <name>...` (names without the `rfsoc4x2_` prefix) writes the XSA here; it fails if timing is not met at speed grade -2 or -1 (below). It needs a Vivado license for the part. `scripts/sim_rf.sh` simulates the RF design's own logic (`hdl/`) and needs none.

## Speed grade

The board's reference manual lists a speed grade -1 chip; Real Digital's board files, and these designs, say -2. They
cannot be built for -1: Vivado rejects the board preset's processor clocks (two at 533 MHz) and memory timing for that
grade. So every build also analyses the routed design with -1 delays and fails if either grade misses
(`scripts/timing_speed.tcl -tclargs -1 <routed.dcp>...` does the same for a checkpoint). All seven designs meet timing
at both. `dma` and `rf_loopback` first missed at -1 (one hold check by 0.001 ns, two setup checks by 0.014 ns); they are
now placed and routed with extra clock uncertainty (`hrx_margins` in their build scripts), which is removed again
before timing is judged. Smallest slack at -1, 2026-11-26:

| Design | Setup (ns) | Hold (ns) |
|---|---|---|
| `gpio` | 6.083 | 0.010 |
| `intc` | 5.230 | 0.002 |
| `loopback` | 3.349 | 0.018 |
| `dma` | 3.757 | 0.037 |
| `hls_vadd` | 5.241 | 0.017 |
| `hls_scale` | 5.306 | 0.008 |
| `rf_loopback` | 0.202 | 0.020 |

Which grade the chip really is has not been established.

## Recording (`rf_loopback`)

One capture records both ADC inputs at once (3.0; before, one at a time, chosen by `ctrl[1]`). In memory each capture
word is 32 bytes: 8 samples of `vin2_01`, then 8 of `vin2_23`, taken on the same clock, 16 bits each at 2457.6 MS/s. Up
to 16384 words (131072 samples, 53 us, per input) fit the FIFO in front of the DMA and lose nothing. The players and the
capture have no common start.

## Probing on a board

`scripts/probe.py <name>...` programs the board over JTAG (JTAG boot mode; volatile) and exercises the design with raw register accesses from `xsdb`, using no Hardroxide code: so a later failure of a driver can be told from a fault of the design. The `gpio` probe also tests DDR in both address ranges.

For the RF loopback, connect SMA cables from DAC A to ADC A and from DAC B to ADC B, and set `HRX_RF_LOOPBACK=1` to require both tones.

| Board connector | In the design |
|---|---|
| DAC A | DAC tile 2, block 0: `vout20`, player 1 |
| DAC B | DAC tile 0, block 0: `vout00`, player 0 |
| ADC A | ADC tile 2, `vin2_23`: bytes 16 to 31 of each capture word |
| ADC B | ADC tile 2, `vin2_01`: bytes 0 to 15 of each capture word |

The DACs were identified by unplugging DAC A's cable; the ADCs follow from the cabling above.

## Third-party files

`board_files/` (Real Digital, Apache 2.0) and `clocks/` (Xilinx RFSoC-PYNQ, BSD 3-Clause); each has its license and origin.
