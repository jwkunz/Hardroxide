# Running on hardware

Several drivers have been run on a board (see the results table in
[Verification gaps](../08-verification-gaps.md#board-results)); the rest have
not. Before relying on anything, work
through [Verification gaps](../08-verification-gaps.md) **in order**: later
checks assume the earlier ones passed.

## One-time board setup

On the stock PYNQ-Z2 image, as root:

- the FPGA manager must exist: `ls /sys/class/fpga_manager/fpga0`;
- `/dev/mem` must be usable by root (it is on the PYNQ image);
- if the image loads its own overlay at boot (the PYNQ-Z2 "base" design, or a
  vendor image that drives HDMI/LEDs from the PL), **stop everything using it
  first**. Reprogramming the PL under a live driver froze a test board
  completely (see [G16](../08-verification-gaps.md)). As root:
  ```console
  # systemctl stop panther-desktop jupyter        # whatever uses the PL on your image
  # rmmod panther_drm panther_audio zocl          # check `lsmod`
  # for d in /sys/bus/platform/devices/4*; do echo $(basename $d) > $d/driver/unbind; done
  ```
- for DMA or HLS pointer arguments: load `u-dma-buf` with one buffer per
  `/dev/udmabufN` you plan to use;
- for interrupts: apply the generated `interrupts.dtso` (see
  [Interrupts](interrupts.md#one-time-setup-on-the-board)).

### Building u-dma-buf

DMA and HLS pointer arguments need the
[`u-dma-buf`](https://github.com/ikwzm/udmabuf) kernel module, which the image
used for validation did not include. Its headers-only `build` directory cannot
build modules on the board, so cross-build on the host against the same kernel:

1. Clone the matching `linux-xlnx` tag (`xilinx-v2024.1` for kernel `6.6.10-xilinx-v2024.1`),
   and copy the board's `/proc/config.gz`, `Module.symvers`,
   `include/generated` and `include/config` from
   `/lib/modules/$(uname -r)/build` over it. The kernel has `CONFIG_MODVERSIONS`,
   so the exported-symbol checksums must come from the board.
2. Set `SUBLEVEL` and `CONFIG_LOCALVERSION` so the release string equals
   `uname -r` exactly (`modinfo -F vermagic` of an installed module shows what is
   required).
3. `make ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf- modules_prepare`, then build
   u-dma-buf **v5.7.1** (older releases do not compile against kernel 6.6) with
   `make -C <linux> M=<udmabuf> CONFIG_U_DMA_BUF=m modules`.
4. Copy the `.ko` over and
   `insmod u-dma-buf.ko udmabuf0=1048576 udmabuf1=1048576`.

Building on the board itself failed: `make modules_prepare` there recursed
forever because the packaged tree lacks the full Kbuild files.

Building and copying binaries is covered in
[Getting started](getting-started.md#build-for-the-board).

## First contact: `board_check`

Every generated crate has a `board_check` example that runs the first checks
in one go and touches no IP register, so it cannot hang on a bad IP:

```console
$ sudo ./board_check            # or: sudo -E env "PATH=$PATH" cargo run --example board_check
```

1. loads the bitstream and applies the clocks (G1, G4);
2. reads every FCLK back from the SLCR and compares it with the design (G2);
3. opens the UIO device of every interrupt that reaches the PS (G11).

It prints one line per step and exits non-zero if any step fails. Fix those
first; nothing else is meaningful until step 1 passes.

## Then, per IP

Run the examples that exist for your design (each is generated only when the
matching IP is present):

| Example | IP | What it does | Gap |
|---|---|---|---|
| `gpio_smoke` | AXI GPIO | walks a one-hot pattern on the outputs, prints the inputs | G5 |
| `hls_smoke` | HLS kernel | prints control status and readable registers; does not start the core | G9 |
| `dma_loopback` | AXI DMA | sends a buffer through a stream loopback and compares | G10 |
| `irq_probe` | any interrupt | opens every interrupt's UIO device; does not wait | G11 |
| `timer_delay` | AXI Timer | a 100 ms hardware delay measured against the system clock | G12 |
| `spi_loopback` | AXI Quad SPI | transfers in the core's internal loopback mode | G12 |
| `i2c_scan` | AXI IIC | probes every 7-bit address and lists the ones that answer | G12 |
| `bram_test` | AXI BRAM Controller | writes a pattern over the whole memory and reads it back | G14 |
| `uart_hello` | AXI UART Lite | sends a greeting, prints what arrives for one second | G14 |
| `fifo_loopback` | AXI4-Stream FIFO | sends a packet and reads it back (stream TX looped to RX) | G15 |

`uart_hello` and `i2c_scan` need external wiring (a TX-to-RX jumper; a device on the
bus). With no wiring, use the loopback fixture (`pynq_z2_loopback.xsa`, see
[Fixtures](../reference/fixtures.md)): its two UARTs and two IIC cores are wired to each
other in the fabric, and `fixtures/board_tests/loopback/{uart_pair,iic_pair}.rs` test them.

When a check passes, record it in
[Verification gaps](../08-verification-gaps.md) with the date and the board
image version.

## Troubleshooting

| Symptom | Likely cause | What to check |
|---|---|---|
| The board freezes on the first register access | PL not programmed, or its clock not running: AXI accesses to an unclocked/unprogrammed PL never complete | `board_check` steps 1 and 2; `cat /sys/class/fpga_manager/fpga0/state` should be `operating` |
| `load()` fails with a permission error | not root | run with `sudo` |
| `Fpga(NotOperating(..))` | the FPGA manager rejected the image | `dmesg` right after the attempt; G1 lists the format assumptions |
| `Fpga(ClockUnreachable { .. })` or a FCLK mismatch in `board_check` | the requested FCLK cannot be derived from the PS PLL within 1% | the PS crystal in the design (`ps_clk_hz` in `bitstream/load.cfg`) must match the board (50 MHz on PYNQ-Z2) |
| `AlreadyLoaded` | a second `Design::load()` in the same process | keep one `Design` and pass references around |
| `IrqError::NotFound { .. }` | the interrupt overlay is not applied, or the node did not bind | apply `interrupts.dtso`; `cat /sys/class/uio/uio*/name` |
| A UIO device returns `EIO` and `dmesg` shows `irq: type mismatch, failed to map hwirq-N` | an earlier overlay mapped that GIC line with the other trigger type (level vs edge); the mapping survives overlay removal | reboot the board before applying the new overlay |
| A UIO device returns `EIO` and `dmesg` is silent | the base design already mapped that GIC line (SPI 29 = `IRQ_F2P[0]` on the validated image) with the other trigger type | move the edge-triggered core to another `IRQ_F2P` bit, or reboot with the base overlay disabled |
| `Fpga(PlInUse(..))` | a kernel driver is bound to a PL address; reprogramming under it can freeze the CPU | stop and unbind it (One-time board setup) |
| `IrqError::ViaController { .. }` | the interrupt goes through an AXI interrupt controller | open the controller's `IRQ_IRQ` and use `AxiIntc::wait` |
| The board freezes during `board_check` step 1 | the PL is in use by another driver (base overlay, HDMI, `zocl`) while it is reprogrammed | stop and unbind them first (One-time board setup); power-cycle to recover |
| `Udmabuf::open` fails | `u-dma-buf` not loaded, or fewer buffers than expected | `ls /dev/udmabuf*` |
| A driver call returns `Timeout` | the IP never signalled completion: wrong clock, unconnected stream, or the core is held in reset | the IP's status registers through `regs()`, and G3 (PL resets) |
