# Linux on Zynq UltraScale+ (preview, not yet run on a board)

**Status:** `hardroxide generate` writes a Linux crate for a Zynq UltraScale+ design. It has been built, type-checked
and run against simulated registers, and compared with the vendor's tools and the PYNQ image, **but never on a board**.
Expect to find faults. The bare-metal and RTOS flavors have been verified ([tutorials](../tutorials/rfsoc/index.md)).

## What the crate does

`Design::load()`, as on the Zynq-7000, then:

1. checks the bitstream's hash and that no kernel driver is bound to a PL address;
2. hands the bitstream to the kernel's `fpga_manager`;
3. sets the widths of the ports between processor and PL to the design's (`bitstream/ports.cfg`);
4. sets the PL clocks (`bitstream/load.cfg`);
5. maps every block through `/dev/mem`.

## The board (RFSoC 4x2, PYNQ 4.0.0 image)

Read from the image, not yet tried:

| Needed | In the image |
|---|---|
| `fpga_manager` for this chip, device-tree overlays, `/dev/mem` | built into the kernel (6.12) |
| interrupts | `uio_pdrv_genirq`, loaded at boot, with `of_id=generic-uio` on the command line |
| the line `pl_ps_irq0[0]` | already a UIO device, `fabric`: the generated crate uses it, and its overlay leaves it out |
| memory for DMA | `u-dma-buf` is **not** shipped; the compiler and kernel build files are, so it can be built as on the Zynq-7000 ([how](hardware.md#building-u-dma-buf); a newer release may be needed for this kernel). The kernel's pool for it is 512 MiB below 2 GiB |

## Building

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --name design_1 -o ../base --hardroxide-path .
$ cd ../base
$ rustup target add aarch64-unknown-linux-gnu
$ CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    cargo build --release --examples --target aarch64-unknown-linux-gnu
```

Copy a program to the board and run it as root, `board_check` first. For interrupts, compile and apply
`interrupts.dtso` as on the Zynq-7000 ([Interrupts](interrupts.md)); the base overlay needs none, its one line being the
image's.
