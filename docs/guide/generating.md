# Generating a crate

```console
$ hardroxide generate <design.xsa> [-o DIR] [--name NAME] [--hardroxide-path ROOT]
```

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
| `vendor:hls:*` (Vitis HLS) | generated wrapper over [`HlsIp`](../reference/ip-hls.md) with named registers |
| anything else | a raw MMIO region (`B::Region`) |

Instances without an address window (interconnects, reset blocks) and the
processing system itself are not exposed.

## Naming

Field and module names are the snake_case of the Vivado instance and register
names. Invalid characters become `_`, Rust keywords get a trailing `_`, and
collisions are resolved in document order with `_2`, `_3`, ….
