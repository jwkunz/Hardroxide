# RFSoC 1: First light

**Board:** [RFSoC 4x2](../../boards/rfsoc-4x2.md) · **Runs:** bare metal

`baremetal/psu-smoke` checks the processor system: clocks, caches, memory, timers and floating point.

```console
$ cd baremetal/psu-smoke
$ cargo build --release
$ hardroxide run target/aarch64-unknown-none/release/psu-smoke \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 20
```

`run` resets the board, programs the FPGA with the XSA's bitstream, runs the XSA's `psu_init` (memory, pins, clocks),
loads the program onto the first Cortex-A53 and shows the console:

```text
== hardroxide-psu smoke test ==
uart reference 99999999 Hz, baud Some(115207)
cpu 1199999988 Hz, counter 33333333 Hz, core 0, EL3
SCTLR 0x30c50830
sleep(100 ms) took 100006 us
caches: SCTLR 0x30c51835; 20 passes over 64 KiB: 131573 us off, 2209 us on; checksums match; dma buffer at 0x00200000, 2 MiB aligned: true
memory test, low DDR: 64 MiB twice at 0x0010000000: 0 errors (208 ms)
memory test, top of low DDR: 64 MiB twice at 0x007c000000: 0 errors (208 ms)
memory test, high DDR: 64 MiB twice at 0x0800000000: 0 errors (208 ms)
memory test, top of high DDR: 64 MiB twice at 0x087c000000: 0 errors (208 ms)
memory test, address lines: 0 errors
non-cacheable block: 0 errors
timer interrupts: 20 in 200168 us (period target 10000 us)
floating point across interrupts: 0 wrong results in 201 rounds
all 32 vector registers whole across 5 interrupts: true
unhandled interrupts: 0
wfi woke on the timer three times
ALL OK
-- executing an undefined instruction on purpose

*** EXCEPTION: undefined instruction
  ELR = 0x0000000000102460  ESR = 0x02000000
```

The last lines are deliberate: they show what an unhandled exception prints (the registers follow).

## What to take from it

- A program runs at **EL3**, alone on core 0, with no firmware under it ([`hardroxide-psu`](../../reference/psu.md)).
- Memory is in two ranges: below 2 GiB, and from `0x8_0000_0000`.
- Turning the caches on made the loop here about 60 times faster.
- A program that does not use the FPGA, like this one, can be run with `--no-pl`, which saves the half minute. The reset
  at the start of a run **clears the FPGA** on this family, so `--no-pl` leaves it empty.

Next: [the base overlay](02-base-overlay.md).
