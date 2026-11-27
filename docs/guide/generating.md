# Generating a crate

```console
$ hardroxide generate <design.xsa> [-o DIR] [--name NAME] [--platform linux|baremetal|rtos] [--hardroxide-path ROOT]
```

`--platform` chooses what the crate runs on ([choosing a flavor](workflow.md#choosing-a-flavor)): `linux`
(default) is a userspace crate for the stock PYNQ image, `baremetal` runs with no operating system on
[`hardroxide-ps7`](../reference/ps7.md), and `rtos` is the bare-metal crate plus the
[`hardroxide-rt`](../reference/rt.md) kernel. The interface (`pac`, the typed drivers, the interface hash) is the
same for all three. Layouts: [Generated crate](../reference/generated-crate.md) (Linux),
[Bare-metal crate](../reference/baremetal.md) and [Generated RTOS crates](../reference/rt.md#generated-rtos-crates).

The output directory defaults to the block design (or `--name`) in the current
directory. Generation is **deterministic**: the same XSA and options always
produce byte-identical output (no timestamps; the `.bit` header, which carries
build timestamps, is not embedded).

## What you get

See [Generated crate](../reference/generated-crate.md) for the full layout. In
short: a `Design<B>` struct with one field per addressable IP instance, a
register layer (`pac`), the embedded bitstream, and `examples/` smoke tests for
the IP Hardroxide recognises.

## Which IP get drivers

| VLNV (version ignored) | Field type |
|---|---|
| `xilinx.com:ip:axi_gpio` | [`AxiGpio`](../reference/ip-gpio.md) |
| `xilinx.com:ip:axi_dma` | [`AxiDma`](../reference/ip-dma.md) |
| `xilinx.com:ip:axi_timer` | [`AxiTimer`](../reference/ip-timer.md) |
| `xilinx.com:ip:axi_iic` | [`AxiIic`](../reference/ip-iic.md) |
| `xilinx.com:ip:axi_quad_spi` | [`AxiSpi`](../reference/ip-spi.md) |
| `xilinx.com:ip:axi_intc` | [`AxiIntc`](../reference/ip-intc.md) |
| `xilinx.com:ip:axi_bram_ctrl` | [`AxiBram`](../reference/ip-bram.md) |
| `xilinx.com:ip:axi_uartlite` | [`AxiUart`](../reference/ip-uart.md) |
| `xilinx.com:ip:axi_fifo_mm_s` | [`AxiFifo`](../reference/ip-fifo.md) |
| `vendor:hls:*` (Vitis HLS) | generated wrapper over [`HlsIp`](../reference/ip-hls.md) with named registers, and 64-bit address/buffer setters for pointer arguments |
| anything else | a raw MMIO region (`B::Region`) |

Instances without an address window (interconnects, reset blocks, `xlconcat`)
and the processing system itself are not exposed. For a raw region you get the
`pac` constants (offsets and field masks) and `read32`/`write32` on the region,
which is enough to drive a custom RTL block by hand.

Some driver constructors need a value from the design (for example the AXI
Timer's clock or an AXI DMA's maximum transfer length). The generator reads
them from the HWH and passes them in; see [Generated crate](../reference/generated-crate.md).

## Checking the result

The generated crate builds on its own (`cargo build`), and `cargo test` runs
nothing hardware-related. Before going to the board, read through
`src/lib.rs` (the `Design` fields tell you what was recognised) and
`interrupts.dtso` (its comments list any interrupt that could not be traced).

## Supported devices

**Zynq-7000** and **Zynq UltraScale+** designs work on every platform. What differs for UltraScale+:
[the bare-metal and RTOS crates](../reference/baremetal.md#zynq-ultrascale) and
[the Linux crate](linux-ultrascale.md), with what has run on a board. `generate` and `update` refuse anything else with an
`unsupported architecture` error; `hardroxide inspect` still reads it.

## Naming

Field and module names are the snake_case of the Vivado instance and register
names. Invalid characters become `_`, Rust keywords get a trailing `_`, and
collisions are resolved in document order with `_2`, `_3`, ….
