# 02 - Architecture

Status: draft, updated through interview round 4.

## Decisions

| Topic | Decision |
|---|---|
| Project name | **Hardroxide** |
| Tool | `hardroxide` CLI; `hardroxide generate <design.xsa>` generates the HAL crate. Also invocable as `cargo hardroxide` via a `cargo-hardroxide` shim. |
| Crate prefix | `hardroxide-*` |
| Repo layout | Single Cargo workspace monorepo: parser, generator, runtime, IP drivers, CLI, examples, and test fixtures together. |
| Generated crate | One crate per design: embedded bitstream, PAC-style register layer, and a typed `Design` struct. It depends on the shared runtime and IP driver crates rather than vendoring them. |
| PS scope | PL only. Linux owns PS peripherals. Hardroxide touches the PS only for PL clocks/resets and the FPGA manager. |

## Planned workspace crates (working names)

| Crate | Role |
|---|---|
| `hardroxide-hwh` | Parse XSA / HWH into a typed design model |
| `hardroxide-gen` | Code generation library (model to Rust crate) |
| `hardroxide-mmio` | MMIO backend trait; `/dev/mem`, UIO, and mock backends |
| `hardroxide-dma` | `DmaBuffer` trait and uncached contiguous backend |
| `hardroxide-irq` | Blocking and async (feature-gated) interrupt waiting |
| `hardroxide-fpga` | Bitstream loading, PL clock/reset control |
| `hardroxide-ip` | Curated drivers keyed by VLNV (GPIO, DMA, Timer, IIC, SPI, HLS) |
| `hardroxide-cli` | The `hardroxide` binary and `cargo-hardroxide` shim |
| `examples/`, `fixtures/` | Sample designs, golden XSAs |

## Pipeline

```
design.xsa --> hardroxide generate --> my_design/ (Cargo crate)
                                         |- design.bit (embedded)
                                         |- src/pac.rs (registers)
                                         `- src/lib.rs (Design, load())
```

## Open questions

- Exact register source details for each curated IP (see 03-generated-crate.md).
- Naming of Rust identifiers derived from Vivado instance names.
