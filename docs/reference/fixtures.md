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
| `pynq_z2_mem_uart.xsa` | AXI BRAM Controller (8 KiB) + AXI UART Lite; UART interrupt on `IRQ_F2P[0]`, UART pins unconstrained | `build_pynq_z2_mem_uart.tcl` |
| `pynq_z2_hls_scale.xsa` | HLS `scale(in, out, n, k)` with two `m_axi` masters into `S_AXI_HP0` (64-bit pointer registers); interrupt on `IRQ_F2P[0]` | `build_pynq_z2_hls_axi.tcl`, `fixtures/hls_scale/` |
| `pynq_z2_intc.xsa` | AXI Timer + IIC + GPIO interrupts combined by an AXI Interrupt Controller (`xlconcat` → `intr`), whose `irq` drives `IRQ_F2P[0]`. GPIO/IIC pins unconstrained | `build_pynq_z2_intc.tcl` |
| `pynq_z2_peripherals.xsa` | AXI Timer + IIC + Quad SPI; three interrupts via `xlconcat`. IIC/SPI pins unconstrained | `build_pynq_z2_peripherals.tcl` |

Rebuild one (Vivado 2025.2 required; not needed to use Hardroxide):

```console
$ source ~/AMD/2025.2/Vivado/settings64.sh
$ vivado -mode batch -source fixtures/scripts/build_pynq_z2_gpio.tcl \
      -tclargs <workdir> fixtures/pynq_z2_gpio.xsa
```

After rebuilding, refresh the golden snapshots with
`UPDATE_GOLDEN=1 cargo test -p hardroxide-gen` and review the diff. See
[HWH findings](../07-hwh-findings.md) for what each export contains.
