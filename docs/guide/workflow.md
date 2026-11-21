# The workflow

This page is the map. Each step names the command and the page with the detail;
[the tutorials](../tutorials/index.md) walk through every step with real output.
The steps below are for the default **Linux** flavor; the other two flavors differ only in steps 3, 5 and 6, listed
in [Choosing a flavor](#choosing-a-flavor).

```text
 Vivado block design
        |  1. export                      write_hw_platform -include_bit   ->  design.xsa
        v
   design.xsa
        |  2. hardroxide inspect          what will I get?
        |  3. hardroxide generate         ->  a Cargo crate
        v
  generated crate  ----- 4. write your program (examples/ or a crate that depends on it)
        |  5. cargo build --target armv7-unknown-linux-gnueabihf
        v
  binary  ----- 6. copy to the PYNQ-Z2, free the PL, run as root
        |
        |  design changed?   7a. same interface:  hardroxide update new.xsa      (swap the bitstream)
        v                    7b. new interface:   hardroxide update ... --regen  (review the diff)
```

## 1. Export an XSA from Vivado

Your design needs a Zynq processing system with the PL clock and AXI master
enabled, IP on the AXI bus with assigned addresses, and interrupts (if any)
going to `IRQ_F2P`. Then **File > Export > Export Hardware > include bitstream**
(or `write_hw_platform -fixed -include_bit`). Details, supported IP and pitfalls:
[Exporting an XSA from Vivado](vivado.md). If you only want to try Hardroxide,
skip this step: `fixtures/` has ready-made XSAs.

## 2. Inspect

```console
$ hardroxide inspect design.xsa
```

Lists the instances the processor can reach, what each becomes (a curated driver, a
generated HLS wrapper, or a raw register window), where each interrupt goes, the PL
clocks, and what it will leave out. Check it matches what you built before going
further. See [Generating a crate](generating.md).

## 3. Generate

```console
$ hardroxide generate design.xsa -o my_design [--hardroxide-path <checkout>]
```

Output is **deterministic** (same XSA, byte-identical crate), so commit the crate
and diff it. Until the runtime crates are published, `--hardroxide-path` points the
crate at a checkout of this repository. See [Generated crate](../reference/generated-crate.md).

## 4. Write your program

The generated crate has a `Design` type with one typed field per IP. Either add a
file to its `examples/` directory (what the tutorials do), or make a separate crate
that depends on it:

```toml
# app/Cargo.toml
[dependencies]
my_design = { path = "../my_design" }
```

```rust,ignore
let d = my_design::Design::load()?;   // programs the FPGA, sets the clocks, maps the IP
d.axi_gpio_0.write(Channel::One, 0b1010)?;
```

See [Using a generated crate](using.md) and the [IP drivers](../reference/ip.md). You
can develop and unit-test most logic **without a board** against the mock backend.

## 5. Cross-build

The board's cores are 32-bit ARM Linux: build for `armv7-unknown-linux-gnueabihf`
with a cross linker (`rustup target add ...`, `apt install gcc-arm-linux-gnueabihf`).
[`scripts/board/run.sh`](../reference/scripts.md) does build, copy and run in one
step. See [Getting started](getting-started.md#build-for-the-board).

## 6. Prepare the board and run

Once per boot: free the PL (`prep.sh`), and, if you use them, apply the interrupt
overlay (`overlay.sh`) and load `u-dma-buf` for DMA. Then run **`board_check` first**:
it programs the FPGA, reads the clocks back and opens every interrupt, touching no IP
register, so a mistake shows up there rather than as a frozen board. See
[Running on hardware](hardware.md).

## 7. Iterate

| You changed | Run | Result |
|---|---|---|
| Logic only (same IP, addresses, registers, interrupts) | `hardroxide update new.xsa --crate-dir my_design` | only `bitstream/` changes; your code does not |
| Anything software sees (an address, a register, an IP, an interrupt, or a setting a driver is built with, such as a clock) | the same, then `--regen` | `update` refuses and prints the diff; `--regen` regenerates `src/` and `examples/` |

See [Updating a bitstream](updating.md) and [Linux 10](../tutorials/linux/10-update.md).

## Choosing a flavor

`hardroxide generate --platform linux|baremetal|rtos` decides what the generated crate runs on. The register layer,
the typed IP drivers and the `Design` type are shared; the runtime underneath differs
([ADR 0003](../decisions/0003-bare-metal-and-rtos-platforms.md)).

| | Linux (default) | Bare metal | RTOS |
|---|---|---|---|
| Runs as | a process under the stock PYNQ Linux | the only software on core 0 | tasks on the `hardroxide-rt` kernel on core 0 |
| Who owns the PS peripherals | Linux | your program ([`hardroxide-ps7`](../reference/ps7.md)) | your program, with the kernel's tick |
| Set up the design with | `Design::load()` (programs the PL through `fpga_manager`) | `Design::init()` (the PL is programmed by `hardroxide run` or the boot image) | `Design::init()` inside a task |
| Interrupts | UIO devices and the device-tree overlay | the GIC directly | the GIC; handlers give semaphores or queues |
| Cross-build | `armv7-unknown-linux-gnueabihf`, `gcc-arm-linux-gnueabihf` | `armv7a-none-eabihf`, no linker needed | `armv7a-none-eabihf` |
| Run on the board | `scripts/board/run.sh` over SSH | `hardroxide run` (JTAG) or `hardroxide package` (SD/QSPI) | the same as bare metal |
| Host tools | Rust and a cross linker | Rust and the Vitis tools | Rust and the Vitis tools |
| Timing | whatever Linux gives you | deterministic, single thread | deterministic, 10 ms loops with microsecond jitter |
| Guide | [Running on hardware](hardware.md) | [Running bare metal](baremetal.md) | [Running bare metal](baremetal.md), [the kernel](../reference/rt.md) |
| Tutorials | [Linux](../tutorials/linux/index.md) | [Bare metal](../tutorials/baremetal/index.md) | [RTOS](../tutorials/rtos/index.md) |

Both bare-metal flavors need the Vitis tools (they warn and stop if missing) and take over the board: Linux is not
running while they do. `hardroxide update` works the same way for all three flavors.

## When something goes wrong

| Symptom | Go to |
|---|---|
| `inspect` or `generate` rejects the XSA | [Exporting an XSA](vivado.md), [Generating a crate](generating.md#supported-devices) |
| the board freezes, `PlInUse`, `StuckChannel`, an interrupt returns `EIO` | [Troubleshooting](hardware.md#troubleshooting) |
| a driver call times out | [Troubleshooting](hardware.md#troubleshooting), then the IP's reference page |
| data from DMA or an HLS kernel is wrong right after switching designs | [G17](../08-verification-gaps.md#g17-the-ps-pl-hp-port-can-wedge-after-many-design-switches) |
