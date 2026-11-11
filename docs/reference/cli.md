# CLI

The `hardroxide` binary (crate `hardroxide-cli`). The help text below is
verified against the real output by the test suite.

```text
$ hardroxide --help
Generate Rust HAL crates from Vivado XSA files

Usage: hardroxide <COMMAND>

Commands:
  generate  Generate a HAL crate from an XSA
  update    Refresh an existing generated crate from a new XSA
  inspect   Print a summary of an XSA: instances, addresses, clocks, hashes
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## `generate`

```text
$ hardroxide generate --help
Generate a HAL crate from an XSA

Usage: hardroxide generate [OPTIONS] <XSA>

Arguments:
  <XSA>

Options:
  -o, --out <OUT>
          Output directory (created if needed). Defaults to the crate name
      --name <NAME>
          Cargo package name (default: block design name)
      --hardroxide-path <HARDROXIDE_PATH>
          Hardroxide checkout; depend on runtime crates by path
  -h, --help
          Print help
```

Writes the crate described in [Generated crate](generated-crate.md). Output is
deterministic. Exit status is non-zero on any error (unreadable XSA, malformed
HWH, I/O failure).

## `update`

```text
$ hardroxide update --help
Refresh an existing generated crate from a new XSA.

Same interface: replaces only the bitstream files. Different interface: prints a diff and exits non-zero unless `--regen`.

Usage: hardroxide update [OPTIONS] <XSA>

Arguments:
  <XSA>

Options:
  -c, --crate-dir <CRATE_DIR>
          The generated crate directory

          [default: .]

      --regen
          Regenerate sources if the interface changed

  -h, --help
          Print help (see a summary with '-h')
```

| Situation | Behaviour | Exit |
|---|---|---|
| Interface and bitstream unchanged | prints `up to date` | 0 |
| Same interface, new bitstream or clocks | rewrites `bitstream/design.bin`, `design.sha256`, `load.cfg` only | 0 |
| Different interface, no `--regen` | prints a line diff of `interface.txt` to stderr, touches nothing | non-zero |
| Different interface, `--regen` | regenerates everything except `Cargo.toml`, prints the diff | 0 |

The comparison is on the canonical interface text (`interface.txt`), not raw
HWH XML, so a Vivado upgrade alone does not force regeneration. See
[`Design::interface_text`](hwh.md#interface-hash).

## `inspect`

```text
$ hardroxide inspect --help
Print a summary of an XSA: instances, addresses, clocks, hashes

Usage: hardroxide inspect <XSA>

Arguments:
  <XSA>

Options:
  -h, --help  Print help
```

Prints the block design name, device, Vivado version, interface hash, each
enabled FCLK, and every address-map entry with its VLNV.
