# How it works

This chapter explains what Hardroxide reads, what it generates, and what
happens at run time, so the rest of the book makes sense. Nothing here is
needed to *use* a generated crate, but it helps when something goes wrong.

## The input: an XSA

Vivado's `write_hw_platform` produces an `.xsa`, which is a zip archive. Hardroxide
reads two things from it:

- the **bitstream** (`.bit`), which configures the programmable logic (PL);
- the **hardware handoff** (`.hwh`), an XML description of the block design:
  every IP instance with its type (VLNV, e.g. `xilinx.com:ip:axi_gpio:2.0`),
  its parameters, its address window, which ports are connected to which, and
  (for Xilinx catalog IP and Vitis HLS kernels) a full register map with field
  names and bit positions. It also records the PS reference clock and the PL
  clock (FCLK) frequencies.

What a real 2025.2 HWH contains, and what it does not, is documented in
[HWH findings](../07-hwh-findings.md).

## The output: a crate

`hardroxide generate` turns that into an ordinary Cargo crate:

| Generated piece | Comes from | What it gives you |
|---|---|---|
| `bitstream/design.bin` | the `.bit`, header stripped and byte-swapped for the Linux FPGA manager | embedded in your binary with `include_bytes!` |
| `bitstream/load.cfg` | PS crystal and FCLK frequencies from the HWH | applied at load time |
| `src/pac.rs` | address map and register maps | one module per IP: base address, size, register offsets, field masks, interrupt lines |
| `src/lib.rs` | IP list + the driver table | `Design<B>`, one typed field per IP, and `load()` |
| `src/ips.rs` | HLS register maps | named setters/getters per HLS kernel |
| `interrupts.dtso` | interrupt wiring | a device-tree overlay exposing each interrupt as a UIO device |
| `examples/` | which IP are present | smoke tests to run on the board |

Each IP becomes a field whose type depends on its VLNV: a curated driver from
[`hardroxide-ip`](../reference/ip.md) for known Xilinx IP, a generated wrapper
for HLS kernels, or a raw register window for anything else (see
[Generating a crate](generating.md#which-ip-get-drivers)).

Generation is deterministic: the same XSA always gives byte-identical output,
so the generated crate can be committed and diffed.

## Two hashes: interface and bitstream

The crate records two fingerprints:

- the **interface hash** covers everything the Rust code depends on: the IP
  instances, their types, addresses, registers and interrupts. Clock
  frequencies and timestamps are excluded.
- the **bitstream hash** is the SHA-256 of `design.bin`.

When you rebuild the FPGA design without changing its interface (a logic fix,
a timing tweak), only the bitstream hash changes, and
[`hardroxide update`](updating.md) swaps the bitstream without touching any
Rust source. If the interface changed, `update` refuses and shows the
difference.

## What `Design::load()` does

On the board, `Design::load()` performs these steps, in order:

1. **Claim the singleton.** Only one `Design` may exist per process, so two
   handles can never drive the same registers. A second `load` fails with
   `AlreadyLoaded` until the first is dropped.
2. **Program the PL** through the Linux FPGA manager
   (`/sys/class/fpga_manager`): the embedded `.bin` is written to
   `/lib/firmware` and the manager is told to load it.
3. **Set the PL clocks** by writing the FCLK dividers in the Zynq's System
   Level Control Registers (SLCR, at `0xF800_0000`) to match the design.
4. **Map every IP** through the chosen MMIO backend (by default `/dev/mem`,
   which needs root) and construct the drivers.

If any step fails, the singleton is released and nothing is left mapped.
Touching a PL address before step 2 succeeds typically hangs the CPU, which
is why `load()` is the only supported way in on real hardware.
`Design::map_only(backend)` skips steps 1–3 and is meant for tests with the
mock backend.

## Backends: where register accesses go

Drivers never touch memory directly; they call `read32`/`write32` on an
[`Mmio`](../reference/mmio.md) region. Three backends provide one:

- `DevMem`: `mmap` of `/dev/mem` (the default for `load()`);
- `Uio`: `mmap` of a `/dev/uioN` device;
- `MockBackend`: a fake memory that records every access, used by all
  hardware-free tests.

All `unsafe` hardware access is confined to the `mmap` code in
`hardroxide-mmio` (and buffer mapping in `hardroxide-dma`); drivers and
generated code are safe Rust.

## Memory for DMA

IP that reads or writes system memory on its own (AXI DMA, HLS kernels with
pointer arguments) needs physically contiguous memory whose physical address
can be handed to the hardware. The default backend is
[`Udmabuf`](../reference/dma.md), backed by the `u-dma-buf` kernel module,
mapped **uncached** so no cache flushing is ever needed. A transfer takes the
buffer by value and only hands it back when the hardware is done, so the
compiler stops you from touching memory the PL is still using.

## Interrupts

PL interrupts reach Linux through the Zynq's interrupt controller (GIC).
Hardroxide traces each IP interrupt pin through the block design to its
`IRQ_F2P` bit, works out the GIC number, and writes a device-tree overlay that
exposes the interrupt as a UIO device. Your code then blocks on that device
(or `await`s it). See [Interrupts](interrupts.md).

## What is and is not verified

Every part of this pipeline that can be checked without a board is checked
against real Vivado 2025.2 output, and the runtime crates' tests also run as
32-bit ARM binaries under emulation. Nothing has yet been run on a PYNQ-Z2;
[Verification gaps](../08-verification-gaps.md) lists every assumption and how
to check it.
