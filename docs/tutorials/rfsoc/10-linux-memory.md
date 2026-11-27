# RFSoC 10: The FPGA's own memory

**Board:** [RFSoC 4x2](../../boards/rfsoc-4x2.md) · **Runs:** Linux

The board has two separate DDR4 memories of 4 GiB: the processor's, where Linux runs, and one wired to the FPGA, which
the base overlay connects to a controller (`ddr4_0`). Linux does not use the second one. A design streams data into it
that would not fit anywhere else; here the processor reads and writes it, through the generated crate.

> **Order matters.** The memory's 200 MHz reference clock comes from the board's clock chip (LMK04828). Until that chip
> has been programmed since power-on, **an access to the memory never completes, and the board has to be switched off**.
> The PYNQ image programs it from `/boot/boot.py` while starting. The program checks that this has happened and refuses
> otherwise; give it two minutes after power-on. (`--clocks-are-programmed` skips the check, for those who program the
> chip another way.)

## Run it

With the crate of [tutorial 8](08-linux.md):

```console
$ export BOARD=xilinx@192.168.2.99
$ scripts/board/run.sh ../base-linux pl_memory
```

```text
== the FPGA's DDR4 ==
30 address lines: ok
4 GiB, appearing again at +4 GiB: true
4 MiB at the start, two patterns: ok
4 MiB at the middle, two patterns: ok
4 MiB at the end, two patterns: ok
from the processor, a word at a time: writes 19.1 MB/s, reads 15.1 MB/s
PL MEMORY OK
```

It **overwrites** what it tests. It samples the memory (12 MiB in three places and one word per address line), which
shows the wiring and the controller work; it is not a test of every cell.

## How it is put together

```rust,ignore
let d = Design::load()?;            // loads the bitstream; the controller then calibrates
std::thread::sleep(Duration::from_millis(500));
d.ddr4_0.write32(offset, value)?;   // a block without a driver is a register window
let back = d.ddr4_0.read32(offset)?;
```

- The generator has no driver for the memory controller, so `ddr4_0` is a plain window (`Mmio`): 32-bit reads and writes
  at an offset, bounds-checked.
- The design maps 8 GiB there and the memory is 4 GiB, so it appears twice. Use offsets below 4 GiB.
- Loading a design restarts the controller: don't count on the contents surviving it.

## Why it is slow from here

Every access goes from the processor through the FPGA to the memory and waits, one word at a time: some 15 to 20 MB/s,
against gigabytes per second for a DMA engine in the FPGA. This memory is for what the FPGA moves, such as the radio
captures; the processor reads the part it needs. Programs that need fast buffers use the processor's memory through
[`u-dma-buf`](../../guide/linux-ultrascale.md#dma-memory-u-dma-buf).

Try this: time `read32` of the same word a million times, and compare it with reading a GPIO register. How much of the
time is the memory, and how much the way there?
