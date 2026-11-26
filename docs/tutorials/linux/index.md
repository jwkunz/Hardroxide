# Linux tutorials

Four hands-on tutorials for the **Linux** flavor (`--platform linux`, the default): your Rust program runs as a Linux
process on the board's ARM cores and manages the programmable logic. All four use the **PYNQ base overlay**, the design
every PYNQ-Z2 already has, so there is one FPGA design and one generated crate to learn. Afterwards,
[Your own design](../own-design/index.md) goes through DMA, HLS kernels, timers and serial buses on small designs of
their own.

```text
  design.xsa  ->  hardroxide generate  ->  cross-build  ->  run on the board
```

You do **not** need Vivado: the design is `fixtures/board/pynq_base_overlay.xsa`. (For the other flavors see
[bare metal](../baremetal/index.md) and [RTOS](../rtos/index.md).)

| # | Tutorial | You learn |
|---|---|---|
| 1 | [LEDs, buttons and switches](01-lights.md) | the whole workflow, end to end |
| 2 | [Interrupts: wait for a button](02-interrupts.md) | the interrupt overlay; the interrupt controller; sleeping until something happens |
| 3 | [Memory inside the FPGA](03-block-ram.md) | block RAM as words and bytes |
| 4 | [The whole overlay](04-base-overlay.md) | what else is in the design, and how Hardroxide shows it |

## The one crate they share

From the root of the checkout, once:

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa -o work/base --hardroxide-path . --name design_1
```

Each tutorial copies a program from `apps/base/linux/` into `work/base/examples/` and runs it.

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
   running system: restarting the board (shut down, then switch off and on) brings
   the desktop back. The password for `sudo` is `xilinx`.

### The commands you will use all the time

```console
$ hardroxide inspect fixtures/board/pynq_base_overlay.xsa     # what is in this design?
$ scripts/board/run.sh work/base <example> [args]             # build, copy, run as root
```

`run.sh` cross-compiles one example of the generated crate, copies it to `$BOARD`, and runs it with `sudo` (it asks for
the password each time unless you set up passwordless `sudo`). Examples are the files in the crate's `examples/`.

### Extras some tutorials need

| Need | Tutorial | How |
|---|---|---|
| Interrupt overlay | 2 | `scp work/base/interrupts.dtso $BOARD:` then `ssh -t $BOARD sudo sh overlay.sh interrupts.dtso` ([why](../../guide/interrupts.md)) |
| Pressing buttons | 1, 2 | the board's real buttons, switches and LEDs |

### If something goes wrong

- **The board stops answering** (SSH times out, nothing on the serial port): switch it off and on. A software `reboot`
  did not recover a board in this state.
- **To put the board back** as it was (the desktop, the image's own drivers): shut it down, then switch it off and on.
  On the PYNQ 3.1 image `reboot` has ended in `Reboot failed -- System halted`.
- **`Fpga(PlInUse(..))`**: run `prep.sh` again.
- **A UIO device returns `EIO`** or **`IrqError::NotFound`**: see [Interrupts](../../guide/interrupts.md) and the table
  in [Running on hardware](../../guide/hardware.md#troubleshooting).
