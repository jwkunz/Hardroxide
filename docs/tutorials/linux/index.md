# Linux tutorials

Eleven hands-on tutorials for the **Linux** flavor (`--platform linux`, the default): your Rust program runs
as a Linux process on the board's ARM cores and manages the programmable logic. There is one tutorial for each test design (fixture) in the
repository. Every tutorial takes the same path you will follow with your own
design:

```text
  design.xsa  ->  hardroxide inspect  ->  hardroxide generate  ->  cross-build  ->  run on the board
```

You do **not** need Vivado: the `.xsa` files are in `fixtures/`. The output
shown in each tutorial was captured on a PYNQ-Z2 (PYNQ 3.1, kernel 6.6); yours
should match except for timings. Do the first one in full; the rest assume you
have. (For the other flavors see [bare metal](../baremetal/index.md) and [RTOS](../rtos/index.md).)

| # | Tutorial | Design (`fixtures/`) | You learn |
|---|---|---|---|
| 1 | [LEDs, buttons and switches](01-gpio.md) | `pynq_z2_gpio.xsa` | the whole workflow, end to end |
| 2 | [Timer, SPI and interrupts](02-timer-spi-irq.md) | `pynq_z2_peripherals.xsa` | waiting for an interrupt; applying the overlay |
| 3 | [UART and I2C without wires](03-uart-iic-loopback.md) | `pynq_z2_loopback.xsa` | two cores wired together inside the FPGA |
| 4 | [An HLS kernel](04-hls-vadd.md) | `pynq_z2_hls_vadd.xsa` | typed registers for a Vitis HLS kernel |
| 5 | [An HLS kernel that reads memory](05-hls-pointers.md) | `pynq_z2_hls_scale.xsa` | pointer arguments and DMA buffers |
| 6 | [AXI DMA](06-dma.md) | `pynq_z2_dma_loopback.xsa` | moving data with ownership-checked buffers |
| 7 | [Many interrupts, one controller](07-interrupt-controller.md) | `pynq_z2_intc.xsa` | AXI interrupt controller fan-in |
| 8 | [Block RAM and UART](08-bram-uart.md) | `pynq_z2_mem_uart.xsa` | memory as a byte buffer |
| 9 | [AXI4-Stream FIFO](09-stream-fifo.md) | `pynq_z2_fifo.xsa` | packets, and why lengths round up |
| 10 | [Updating a bitstream](10-update.md) | `variants/` | swapping a rebuilt design without touching code |
| 11 | [A design you did not build](11-base-overlay.md) | `real/pynq_base_overlay.xsa` | the PYNQ base overlay: 70 IP, one command |

## Before you start (once)

### On your workstation

You need Rust (stable), a checkout of this repository, and a cross linker for
the board's 32-bit ARM cores:

```console
$ git clone https://github.com/jwkunz/Hardroxide && cd Hardroxide
$ cargo build --release -p hardroxide-cli
$ export PATH=$PWD/target/release:$PATH          # puts `hardroxide` on your PATH
$ rustup target add armv7-unknown-linux-gnueabihf
$ sudo apt install gcc-arm-linux-gnueabihf        # Debian/Ubuntu; or set LINKER (see below)
$ hardroxide --help
```

The tutorials run every command **from the root of the checkout** and put
generated crates under `work/` (ignored by git). `scripts/board/run.sh` finds the
cross linker itself (`arm-linux-gnueabihf-gcc` on your `PATH`, otherwise the one
bundled with Vitis); set `LINKER=/path/to/arm-linux-gnueabihf-gcc` to override.

### On the board

1. Power the PYNQ-Z2, connect Ethernet, and make sure you can log in. The stock
   image's user is `xilinx` with password `xilinx`:

   ```console
   $ export BOARD=xilinx@pynq          # or xilinx@<its IP address>
   $ ssh $BOARD uname -m
   armv7l
   ```

2. Copy the helper scripts to the board:

   ```console
   $ scp scripts/board/*.sh $BOARD:
   ```

3. **Free the programmable logic.** The stock image keeps drivers and services
   attached to the PL (the HDMI display and the board's GPIOs, for example).
   Reprogramming the PL underneath a live driver can freeze the CPU, which
   Hardroxide refuses to do (`PlInUse`). Stop them with:

   ```console
   $ ssh -t $BOARD sudo sh prep.sh
   stopped service jupyter
   removed module panther_audio
   removed module zocl
   unbound 41200000.gpio (gpio-xilinx)
   ...
   ready: no driver is bound to a PL address
   ```

   Run this **once per boot, a couple of minutes after the board has finished
   starting**: services that start late can bind drivers again, and then the
   next step fails with `PlInUse`; just run `prep.sh` again. It only changes the
   running system; a reboot brings the desktop back. The password for `sudo` is
   `xilinx`.

### The three commands you will use all the time

```console
$ hardroxide inspect fixtures/<design>.xsa                    # what is in this design?
$ hardroxide generate fixtures/<design>.xsa -o work/<name> --hardroxide-path .
$ scripts/board/run.sh work/<name> <example> [args]           # build, copy, run as root
```

`run.sh` cross-compiles one example of the generated crate, copies it to
`$BOARD`, and runs it with `sudo` (it asks for the password each time unless you
set up passwordless `sudo`). Examples are the files in the crate's `examples/`;
the tutorials add your own by copying a program from `fixtures/board_tests/`
there.

### Extras some tutorials need

| Need | Tutorials | How |
|---|---|---|
| Interrupt overlay | 2, 3, 4, 5, 6, 7, 9 | `scp work/<name>/interrupts.dtso $BOARD:` then `ssh -t $BOARD sudo sh overlay.sh interrupts.dtso` ([why](../../guide/interrupts.md)) |
| `u-dma-buf` kernel module | 5, 6 | [Building u-dma-buf](../../guide/hardware.md#building-u-dma-buf), then `sudo insmod u-dma-buf.ko udmabuf0=1048576 udmabuf1=1048576` |
| Pressing buttons | 1 | none; the design uses the board's real buttons and LEDs |
| Wiring | none | all other designs work without wires |

### If something goes wrong

- **The board stops answering** (SSH times out, nothing on the serial port):
  power-cycle it. A software `reboot` did not recover a board in this state.
- **`Fpga(PlInUse(..))`**: run `prep.sh` again.
- **A UIO device returns `EIO`** or **`IrqError::NotFound`**: see
  [Interrupts](../../guide/interrupts.md) and the table in
  [Running on hardware](../../guide/hardware.md#troubleshooting).
- **A DMA or HLS transfer loses its last bytes, or `StuckChannel`** right after a
  design switch: a known board-state problem, see
  [Known issue: wedged HP port](../../08-verification-gaps.md#g17-the-ps-pl-hp-port-can-wedge-after-many-design-switches).
