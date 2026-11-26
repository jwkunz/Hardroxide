# Fixtures

Real Vivado 2025.2 exports used for golden-snapshot tests and for checking
register maps, committed under `fixtures/` with their build scripts in
`fixtures/scripts/`. All target the XC7Z020 (PYNQ-Z2) with PS7 FCLK0 = 100 MHz
and a 50 MHz crystal, with no board preset (DDR/MIO settings are not
board-accurate; the PL side is).

| Fixture | Contents | Script |
|---|---|---|
| `pynq_z2_gpio.xsa` | AXI GPIO: 4 LEDs out, 4 buttons + 2 switches in | `build_pynq_z2_gpio.tcl` |
| `pynq_z2_hls_vadd.xsa` | HLS `vadd` (`a + b`) with interrupt on `IRQ_F2P` | `build_pynq_z2_hls.tcl`, `fixtures/hls/` |
| `pynq_z2_dma_loopback.xsa` | AXI DMA looped back through an AXIS FIFO; two interrupts via `xlconcat` | `build_pynq_z2_dma.tcl` |
| `pynq_z2_fifo.xsa` | AXI4-Stream FIFO with its stream transmit port looped to its receive port; interrupt on `IRQ_F2P[0]` | `build_pynq_z2_fifo.tcl` |
| `pynq_z2_loopback.xsa` | two AXI UART Lite and two AXI IIC cores wired to each other inside the fabric (no pins; custom top level); interrupts on `IRQ_F2P[3:0]` | `build_pynq_z2_loopback.tcl` |
| `pynq_z2_mem_uart.xsa` | AXI BRAM Controller (8 KiB) + AXI UART Lite; UART interrupt on `IRQ_F2P[0]`, UART pins unconstrained | `build_pynq_z2_mem_uart.tcl` |
| `pynq_z2_hls_scale.xsa` | HLS `scale(in, out, n, k)` with two `m_axi` masters into `S_AXI_HP0` (64-bit pointer registers); interrupt on `IRQ_F2P[0]` | `build_pynq_z2_hls_axi.tcl`, `fixtures/hls_scale/` |
| `pynq_z2_intc.xsa` | AXI Timer + IIC + GPIO interrupts combined by an AXI Interrupt Controller (`xlconcat` → `intr`), whose `irq` drives `IRQ_F2P[0]`. GPIO/IIC pins unconstrained | `build_pynq_z2_intc.tcl` |
| `pynq_z2_peripherals.xsa` | AXI Timer + IIC + Quad SPI; three interrupts via `xlconcat`. IIC/SPI pins unconstrained | `build_pynq_z2_peripherals.tcl` |

Variants for the update tutorial are in `fixtures/variants/`: `pynq_z2_peripherals_rev2.xsa`
is the peripherals design rebuilt with a different bitstream `USERID` (same interface:
`hardroxide update` accepts it), and `pynq_z2_peripherals_50mhz.xsa` has FCLK0 at 50 MHz
and FCLK1 at 142.857 MHz (a different interface: `update` refuses it without `--regen`).
They come from `build_pynq_z2_peripherals.tcl` with, respectively,
`set_property BITSTREAM.CONFIG.USERID 0xC0FFEE42 [current_design]` added to the
write-bitstream pre-hook and `PCW_FPGA0_PERIPHERAL_FREQMHZ {50}` /
`PCW_EN_CLK1_PORT {1}` / `PCW_FPGA1_PERIPHERAL_FREQMHZ {142.857}` in the PS7 settings.
Real designs from other sources are in `fixtures/real/` (see its README).

`fixtures/board/pynq_base_overlay.xsa` is the PYNQ base overlay with a `ps7_init` ([PYNQ base overlay](base-overlay.md)); the programs for it are in `apps/base/`.

`fixtures/board/` holds the same designs rebuilt with the PYNQ-Z2 board preset (DDR, MIO and the PS UART as on the real
board). The bare-metal and RTOS flavors need these, because `ps7_init` takes the console pins and clocks from the XSA; the
plain fixtures above leave the PS UART disabled, so a bare-metal program built from them prints nothing.

The [tutorials](../tutorials/index.md) start on the base overlay, with programs from `apps/base/`; the tutorials of
[Your own design](../tutorials/own-design/index.md) each use one of these designs and programs in
`fixtures/board_tests/` (Linux) and `fixtures/board_tests_bm/` (bare metal), which also serve as driver tests. The tests
`--test apps`, `--test board_tests` and `--test baremetal` of `hardroxide-gen` build every one of those programs against
a freshly generated crate, so the tutorial code cannot drift from the API.

Rebuild one (Vivado 2025.2 required; not needed to use Hardroxide):

```console
$ source ~/AMD/2025.2/Vivado/settings64.sh
$ vivado -mode batch -source fixtures/scripts/build_pynq_z2_gpio.tcl \
      -tclargs <workdir> fixtures/pynq_z2_gpio.xsa
```

After rebuilding, refresh the golden snapshots with
`UPDATE_GOLDEN=1 cargo test -p hardroxide-gen` and review the diff. See
[HWH findings](../07-hwh-findings.md) for what each export contains.

The bare-metal and RTOS tutorial programs are in `fixtures/board_tests_bm/{baremetal_gpio,rtos_gpio}/`
(`cargo test -p hardroxide-gen --test baremetal` builds them for the target), and the kernel's own programs in the
separate `baremetal/` workspace (`ps7-smoke`, `rt-smoke` for QEMU and the board, `rt-jitter` for the long timing run).

`fixtures/board_tests/` holds the on-board test programs (see its README); `board_tests/loopback/{uart_pair,iic_pair}.rs` test the loopback fixture (generate its crate, copy them into `examples/`, build for the board). The UART pair exchanges 256 bytes each way; the IIC pair makes core 1 a target at `0x2A` (its `ADR` register, offset `0x110`), writes five bytes from core 0, and scans the bus.

## RFSoC 4x2 base overlay

`fixtures/board/rfsoc4x2_base_overlay.xsa` is made from RFSoC-PYNQ's SD card image (BSD 3-Clause, see
`fixtures/board/LICENSE.rfsoc-pynq`):

```console
$ source ~/AMD/2025.2/Vivado/settings64.sh
$ vivado -mode batch -source fixtures/rfsoc4x2/scripts/build_base_ps.tcl -tclargs <workdir> <ps-only.xsa>
$ python3 fixtures/scripts/package_rfsoc4x2_base.py RFSoC4x2-4.0.0.img <ps-only.xsa>
```

The first builds a design of the processor alone (the board preset plus the overlay's processor-to-FPGA settings) for its
`psu_init`, and needs no license; the second copies `base.bit` and `base.hwh` out of the image (read only, without root)
and packs the three together. The overlay's own processor settings are not used: they describe 8 GiB of processor memory,
and the preset's, proven on the board, 4 GiB.

The other RFSoC 4x2 designs (`fixtures/rfsoc4x2/`) are driver tests with their blocks wired to each other.

