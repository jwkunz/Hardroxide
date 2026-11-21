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
  run       Run a bare-metal program on a board over JTAG (needs the Vitis tools)
  doctor    Check that the host can drive a board: Vitis tools, JTAG cable access, console port
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
      --platform <PLATFORM>
          Kind of crate: linux (default), baremetal or rtos [default: linux]
  -h, --help
          Print help
```

Writes the crate described in [Generated crate](generated-crate.md). Output is
deterministic. `--platform baremetal` writes a `no_std` crate for core 0 with no
operating system ([Bare-metal crate](baremetal.md)); `rtos` is reserved and fails with
a clear message until the kernel exists. Exit status is non-zero on any error (unreadable XSA, malformed
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
enabled FCLK, and then every instance the PS can reach, in address order: its
base address, size, the field name it gets on the generated `Design`, what it
becomes (a curated driver from `hardroxide-ip`, a generated HLS wrapper, or a
raw register window), its VLNV and each connected interrupt with where it goes
(`IRQ_F2P[n]` with its GIC number, or an input of an AXI interrupt controller).
It ends with a count by kind and the instances that get no field because they
are private to another master (a MicroBlaze, for example). It uses the same
model as `generate`, so what it lists is what `generate` produces.

## `run`

```text
$ hardroxide run --help
Run a bare-metal program on a board over JTAG (needs the Vitis tools).

Resets the board, runs the XSA's own ps7_init, programs the PL, loads the ELF, starts it and shows the UART console. The board must be in JTAG boot mode.

Usage: hardroxide run [OPTIONS] --xsa <XSA> <ELF>

Arguments:
  <ELF>
          The program (an ELF built for armv7a-none-eabihf)

Options:
      --xsa <XSA>
          The XSA of the design it runs on (supplies ps7_init and the bitstream)

      --tty <TTY>
          Console serial port
          
          [default: /dev/ttyUSB1]

      --watch <WATCH>
          Seconds to show the console after the program starts
          
          [default: 10]

      --no-pl
          Do not program the PL (the design has none, or it is already configured)

  -h, --help
          Print help (see a summary with '-h')
```

Runs a bare-metal program on a board over JTAG, with the vendor's tools. It resets the
board, runs the XSA's own `ps7_init.tcl` (read from the XSA at run time), programs the PL
from the XSA's bitstream, loads the ELF, starts it and shows the UART console for `--watch`
seconds; the program keeps running afterwards. The board must be in JTAG boot mode, and the
XSA must come from a design with a board-accurate PS configuration, or the console prints
nothing. `hw_server` is started if it is not running. If the Vitis tools are not found
(`HRX_VITIS`, `XILINX_ROOT`, `PATH`, or a standard install location) the command prints what
to do and stops. See [Running bare metal](../guide/baremetal.md).

## `doctor`

```text
$ hardroxide doctor --help
Check that the host can drive a board: Vitis tools, JTAG cable access, console port

Usage: hardroxide doctor [OPTIONS]

Options:
      --tty <TTY>  Console serial port [default: /dev/ttyUSB1]
  -h, --help       Print help
```

Checks the Vitis tools, access to the JTAG cable (the FT2232, `0403:6010`), the console port
and `hw_server`, one line each, and exits non-zero if something is missing, with what to do.
