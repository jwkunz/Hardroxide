# Tutorial 10: Updating a bitstream

**Designs:** `fixtures/pynq_z2_peripherals.xsa` and two variants in
`fixtures/variants/` · **Time:** 15 minutes · **Needs:**
[setup](index.md#before-you-start-once)

FPGA development is a loop: change the logic, rebuild in Vivado, test. Most rebuilds
**do not change what software sees**: the same IP at the same addresses with the same
registers. For those, Hardroxide swaps only the bitstream and leaves your Rust
code and the generated crate untouched. This tutorial shows the loop and its safety
rail.

The two variants are rebuilds of the Tutorial 2 design:

| File | What changed |
|---|---|
| `variants/pynq_z2_peripherals_rev2.xsa` | only the bitstream header's `USERID` (a stand-in for "I fixed some logic"): same interface |
| `variants/pynq_z2_peripherals_50mhz.xsa` | the PL clock FCLK0 is 50 MHz instead of 100 MHz (and FCLK1 runs at 142.857 MHz): a different interface |

## 1. Two hashes

Every generated crate records two fingerprints (see [How it works](../guide/concepts.md#two-hashes-interface-and-bitstream)):
an **interface hash** (everything the Rust code depends on) and a **bitstream
hash**. `inspect` prints the first:

```console
$ hardroxide inspect fixtures/pynq_z2_peripherals.xsa | grep 'interface hash' | cut -c1-24
interface hash: 5ffd7edd
$ hardroxide inspect fixtures/variants/pynq_z2_peripherals_rev2.xsa | grep 'interface hash' | cut -c1-24
interface hash: 5ffd7edd
$ hardroxide inspect fixtures/variants/pynq_z2_peripherals_50mhz.xsa | grep 'interface hash' | cut -c1-24
interface hash: d2e12964
```

`rev2` has the same interface hash; `50mhz` does not.

## 2. Generate and run the original

```console
$ hardroxide generate fixtures/pynq_z2_peripherals.xsa -o work/upd --hardroxide-path .
$ cut -c1-16 work/upd/bitstream/design.sha256
1ae871c4207f792f
$ scripts/board/run.sh work/upd timer_delay
100000000 Hz timer: asked 100 ms, took 100.033822ms
$ scripts/board/run.sh work/upd spi_loopback
SPI loopback OK: 40 bytes
```

## 3. Update with the rebuilt design

```console
$ hardroxide update fixtures/variants/pynq_z2_peripherals_rev2.xsa --crate-dir work/upd
interface unchanged; bitstream replaced
  1ae871c4207f792f7c585c13306b346537ada845d3d450b4afe03a24d33bb517
  d25079f51b1233a316dc9c0ced75b6a4754197f2cfdbc894484084ce4f01563d
$ cut -c1-16 work/upd/bitstream/design.sha256
d25079f51b1233a3
$ scripts/board/run.sh work/upd timer_delay
100000000 Hz timer: asked 100 ms, took 100.029038ms
$ scripts/board/run.sh work/upd spi_loopback
SPI loopback OK: 40 bytes
```

The bitstream hash changed; nothing else did. `src/`, `Cargo.toml` and your examples
are byte-for-byte what they were: only `bitstream/design.bin`, `design.sha256`
and `load.cfg` were replaced. Running `update` with the same file again prints
`up to date`.

## 4. The safety rail

Now try the design whose clock is different:

```console
$ hardroxide update fixtures/variants/pynq_z2_peripherals_50mhz.xsa --crate-dir work/upd
interface changed:
-  config axi_clk_hz=100000000
- instance rst_ps7_0_100M xilinx.com:ip:proc_sys_reset:5.0
+  config axi_clk_hz=50000000
+ instance rst_ps7_0_50M xilinx.com:ip:proc_sys_reset:5.0
Error: rerun with --regen to regenerate the crate sources
```

`update` **refused and changed nothing**. Why is a clock part of the interface?
Because the generated `Timer` driver is constructed with the clock frequency it
uses to convert nanoseconds to ticks; a new clock with the old constant would make
every delay twice as long, silently. Hardroxide hashes every parameter its
generated constructors use, so that mistake becomes a loud diff.

Accept the change with `--regen`, which regenerates `src/` and `examples/` (keeping
your `Cargo.toml`):

```console
$ hardroxide update fixtures/variants/pynq_z2_peripherals_50mhz.xsa --crate-dir work/upd --regen
interface changed; crate regenerated:
-  config axi_clk_hz=100000000
- instance rst_ps7_0_100M xilinx.com:ip:proc_sys_reset:5.0
+  config axi_clk_hz=50000000
+ instance rst_ps7_0_50M xilinx.com:ip:proc_sys_reset:5.0
$ cat work/upd/bitstream/load.cfg
ps_clk_hz=50000000
fclk0=50000000
fclk1=142857132
$ scripts/board/run.sh work/upd board_check
1. loading bitstream and applying clocks ...
   ok
2. FCLK readback (SLCR) vs. the design:
   FCLK0: 50000000 Hz (want 50000000) ok
   FCLK1: 142857142 Hz (want 142857132) ok
$ scripts/board/run.sh work/upd timer_delay
50000000 Hz timer: asked 100 ms, took 100.045689ms
```

The board's clock generator was reprogrammed to the new frequencies (read back from
the chip), and the Timer, now built for 50 MHz, still measures a correct 100 ms.
(Step 3 of `board_check` lists interrupt devices; it needs the overlay.)

## Faster iterations on the board

While developing you can skip even the copy-and-rebuild: enable the crate's
`dev-bitstream` feature and call `Design::load_with_bitstream("new.xsa", ...)`, which
loads the bitstream from an XSA at run time (and refuses if the interface differs).
See [Updating a bitstream](../guide/updating.md). Do not enable it in a release build.

## What you did

You rebuilt hardware without touching software, saw `update` replace exactly the
bitstream, and saw it refuse a change that would have silently broken a driver.

Continue with [Tutorial 11: A design you did not build](11-base-overlay.md).
