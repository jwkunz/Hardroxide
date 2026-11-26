# Linux on Zynq UltraScale+ (preview)

**Status:** on an RFSoC 4x2 with the PYNQ 4.0.0 image, generated crates have driven the base overlay and the six test
designs other than the radio one: registers, interrupts, DMA and both HLS kernels
([results](../decisions/0005-ultrascale-plus.md)). The radio blocks are untouched. To start with the base overlay, follow
the tutorials: [first programs](../tutorials/rfsoc/08-linux.md), [a web page](../tutorials/rfsoc/09-linux-web.md),
[the FPGA's memory](../tutorials/rfsoc/10-linux-memory.md).

## What the crate does

`Design::load()`, as on the Zynq-7000, then:

1. checks the bitstream's hash and that no kernel driver is bound to a PL address;
2. hands the bitstream to the kernel's `fpga_manager`;
3. sets the widths of the ports between processor and PL to the design's (`bitstream/ports.cfg`);
4. sets the PL clocks (`bitstream/load.cfg`);
5. maps every block through `/dev/mem`.

## The board (RFSoC 4x2, PYNQ 4.0.0 image)

| Needed | In the image | Tried |
|---|---|---|
| `fpga_manager` for this chip, device-tree overlays, `/dev/mem` | built into the kernel (6.12) | yes |
| interrupts | `uio_pdrv_genirq`, loaded at boot, with `of_id=generic-uio` on the command line | yes |
| the line `pl_ps_irq0[0]` | already a UIO device, `fabric@A0000000`: the generated crate uses it, and its overlay leaves it out | yes |
| memory for DMA and HLS pointers | `u-dma-buf` is **not** shipped: build and load it ([below](#dma-memory-u-dma-buf)). The kernel's pool for it is 512 MiB below 2 GiB | yes |

**Wait about two minutes after power-on.** The image's `/boot/boot.py` programs the clock chips and loads the base
overlay while starting (it was observed some 90 seconds after power-on), which replaces whatever you loaded before.

Nothing else needs to be stopped before loading: the only driver the image binds in the PL's addresses is that UIO device,
which the loader allows. A notebook that is using an overlay should be shut down first, since a new bitstream replaces it.

## Building and running

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --name design_1 -o ../base --hardroxide-path .
$ cd ../base
$ rustup target add aarch64-unknown-linux-gnu
$ CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    cargo build --release --examples --target aarch64-unknown-linux-gnu
$ scp target/aarch64-unknown-linux-gnu/release/examples/board_check xilinx@<board>:
$ ssh -t xilinx@<board> sudo ./board_check
```

```text
1. loading bitstream and applying clocks ...
   ok
2. PL clock readback vs. the design:
   pl_clk0: 99999990 Hz (want 99999985) ok
   pl_clk1: 299999970 Hz (want 299999939) ok
3. interrupt UIO devices (needs interrupts.dtso applied):
   axi_intc_0.irq: ok
all checks passed; next run the per-IP examples
```

Run `board_check` first. For interrupts, compile `interrupts.dtso` on your computer (the image has no `dtc`:
`dtc -@ -I dts -O dtb -o interrupts.dtbo interrupts.dtso`), copy it over and apply it with
[`overlay.sh`](../reference/scripts.md) as on the Zynq-7000 ([Interrupts](interrupts.md)); the base overlay needs none,
its one line being the image's. The programs used for the
board results are in `fixtures/board_tests_rfsoc_linux/`.

Two cautions:

- **Reboot between designs that disagree about an interrupt line.** The kernel keeps the trigger type a line first had.
  If a later overlay declares the other type, it logs `irq: type mismatch`, and that UIO device opens (so `board_check`
  passes it) but using it fails with `Input/output error`.
- The generated `dma_loopback` example is for loopback designs. Don't run it on the base overlay: its DMA engines are
  wired to the radios and the network port, in clock domains that nothing there has started.

## DMA memory (`u-dma-buf`)

[`u-dma-buf`](https://github.com/ikwzm/udmabuf) v5.7.1 builds on the board, from the image's own kernel build directory
(`/lib/modules/$(uname -r)/build`), with two adjustments:

- The image has no `bc`, which the build asks for one header (`kernel/time/timeconst.bc`). Install it, or make the
  header on another machine and put a program named `bc` in the `PATH` that prints it.
- Pass the suffix of the running kernel's version, or the module will be refused.

Copy the module's source to the board (it needs no network there), then:

```console
$ cp -a /lib/modules/$(uname -r)/build /tmp/kbuild
$ V=$(uname -r) && L=${V#*-xilinx}
$ make -C /tmp/kbuild LOCALVERSION=$L modules_prepare
$ cd udmabuf && make KERNEL_SRC=/tmp/kbuild LOCALVERSION=$L all
$ sudo insmod u-dma-buf.ko udmabuf0=2097152 udmabuf1=2097152 udmabuf2=2097152 udmabuf3=2097152
```

On this processor the module's default way of mapping a buffer allows only aligned accesses. `Udmabuf::open` selects its
write-combining mode instead ([why](../reference/dma.md)); a program of your own that opens `/dev/udmabufN` has to do the
same.

Measured with the loopback design (from starting both channels to both interrupts reaching the program):

| | 1 MiB there and back |
|---|---|
| DMA 0 (32-bit stream) | 2.8 ms |
| DMA 1 (128-bit stream) | 0.75 to 0.78 ms |
| HLS `scale`, 65,536 integers (to its interrupt) | 0.80 to 0.84 ms |
