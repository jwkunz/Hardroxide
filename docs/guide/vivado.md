# Exporting an XSA from Vivado

Hardroxide reads an **XSA**: the archive Vivado writes for a finished block design,
containing the bitstream and the hardware handoff (`.hwh`, an XML description of
every IP and its registers). You do not need Vivado to use Hardroxide, only to make
your own designs. Without one, use the XSAs in `fixtures/`.

The designs in `fixtures/` were built with the Tcl scripts in `fixtures/scripts/`;
they are complete, working recipes for everything below.

## What the design needs

**Target.** A Zynq-7000 device. The fixtures use the PYNQ-Z2's `xc7z020clg400-1`
(PS crystal 50 MHz). A Zynq UltraScale+ design is accepted too (the processing system is
`zynq_ultra_ps_e`, the AXI masters `M_AXI_HPM*`, interrupts `pl_ps_irq0` and `pl_ps_irq1`; the
recipes in `fixtures/rfsoc4x2/scripts/` show it); other architectures are refused. The rest of
this page names the Zynq-7000's ports.

**Processing system.** Add the *ZYNQ7 Processing System* and enable what your design
uses. In the fixtures:

| You want | Enable in the PS7 |
|---|---|
| a PL clock | `FCLK_CLK0` (and its frequency, for example 100 MHz); also `FCLK_RESET0_N` |
| the processor to reach your IP | `M_AXI_GP0` (the address window `0x4000_0000` and up) |
| interrupts from the PL | `IRQ_F2P` (use an `xlconcat` to combine several, up to 16) |
| your IP to read or write system memory (DMA, an HLS kernel with pointers) | an `S_AXI_HP` port, usually `S_AXI_HP0` |

**IP on the bus.** Put the IP on the `M_AXI_GP0` interconnect and run **Run
Connection Automation**, then assign addresses in the **Address Editor** (Tcl:
`assign_bd_address`). Every instance the processor should reach needs an address
window; instances without one are not exposed.

**Interrupts.** Connect each interrupt output to `IRQ_F2P` directly, through an
`xlconcat`, or through an AXI interrupt controller whose `irq` goes to `IRQ_F2P`.
Hardroxide traces all three. An interrupt it cannot trace is listed in
`interrupts.dtso` as skipped.

**Pins.** Constrain any pin that leaves the chip. A design with unconstrained pins
fails Vivado's DRC at bitstream time (the loopback fixture avoids pins entirely).

## Which IP get a driver

AXI GPIO, DMA, Timer, IIC, Quad SPI (standard mode), UART Lite, BRAM Controller,
Interrupt Controller and AXI4-Stream FIFO get curated drivers; Vitis HLS kernels get
generated wrappers; anything else becomes a raw register window you can drive by
hand. The full table is in [Generating a crate](generating.md#which-ip-get-drivers).
HLS kernels need `s_axilite` for control and arguments; pointer arguments need
`m_axi` with `offset=slave`. AXI-Stream and array ports on a kernel are not
supported yet.

## Export

In the GUI, after **Generate Bitstream**:

> **File > Export > Export Hardware**, choose **Fixed**, tick **Include bitstream**,
> and save the `.xsa`.

In Tcl (what the fixture scripts do):

```tcl
write_hw_platform -fixed -include_bit -force design.xsa
```

Make sure the XSA includes the bitstream: Hardroxide fails without one.

## Check it

```console
$ hardroxide inspect design.xsa
```

You should see your PL clock, every instance you expect with the type you expect,
and each interrupt going where you wired it. If an instance you expected is
missing, it has no address window (check the Address Editor); if it appears as a
raw register window, it has no curated driver. See
[Generating a crate](generating.md).

## Versions

Vivado/Vitis **2025.2** is what the fixtures were built with and what has been run
on hardware. The parser also reads handoff files from older versions (2019.2 and
later were tried) but those were not run on hardware.

## A minimal example to copy

`fixtures/scripts/build_pynq_z2_gpio.tcl` is about 40 lines: a PS7 with FCLK0 at
100 MHz and `M_AXI_GP0`, one AXI GPIO connected with Connection Automation, ports
for LEDs and buttons with pin constraints, a bitstream run, and the export.
The HLS kernels the fixtures use are in `fixtures/hls/` and `fixtures/hls_scale/`
(with `build_ip.tcl` showing how a kernel becomes an IP-catalog IP).
