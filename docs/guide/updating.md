# Updating a bitstream

A rebuilt bitstream with the **same hardware interface** (same IP, addresses,
registers, interrupts) can be swapped in without regenerating or editing any
Rust code:

```console
$ hardroxide update new_design.xsa --crate-dir my_design
interface unchanged; bitstream replaced
  <old sha256>
  <new sha256>
```

Only `bitstream/design.bin`, `bitstream/design.sha256` and
`bitstream/load.cfg` (clock targets) can change. If the interface differs,
nothing is touched and the command prints a line diff and fails:

```console
$ hardroxide update changed.xsa --crate-dir my_design
interface changed:
- range axi_gpio_0 Reg 0x41200000 0x4120ffff
+ range axi_gpio_0 Reg 0x43000000 0x4300ffff
Error: rerun with --regen to regenerate the crate sources
```

`--regen` regenerates `src/`, `examples/` and the data files but keeps your
existing `Cargo.toml`.

## Iterating on the board without recompiling

Enable the crate's `dev-bitstream` feature and load an XSA at runtime. It must
have the same interface hash as the one the crate was compiled for:

```rust,ignore,fixture=pynq_z2_gpio
# use my_design::{Design, LoadOptions};
# use hardroxide_mmio::DevMem;
let d = Design::load_with_bitstream("new_design.xsa", DevMem::new(), &LoadOptions::default())?;
```

Do not enable `dev-bitstream` in release builds.
