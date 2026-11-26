# Running on hardware

This page is for the **Linux** flavor on the stock PYNQ image. For no operating system or the real-time kernel, see
[Running bare metal and RTOS](baremetal.md).

Most drivers have been run on a PYNQ-Z2 (see the results table in
[Verification gaps](../08-verification-gaps.md#board-results)); the checks that
remain are listed there. If you want a guided run through real designs with real output,
use the [tutorials](../tutorials/index.md); this page is the reference for setting up
the board and fixing problems.

## One-time board setup

On the stock PYNQ-Z2 image, as root:

- the FPGA manager must exist: `ls /sys/class/fpga_manager/fpga0`;
- `/dev/mem` must be usable by root (it is on the PYNQ image);
- **free the PL.** The stock image keeps services and drivers attached to the
  programmable logic (the HDMI display, the board's GPIOs, `zocl`). Reprogramming
  the PL under a live driver froze a test board completely (see
  [G16](../08-verification-gaps.md)), and `load()` refuses to do it (`PlInUse`).
  Run [`scripts/board/prep.sh`](../reference/scripts.md) on the board, as root, once
  per boot **after it has finished starting** (late services can bind drivers again;
  if `PlInUse` appears, run it again):

  ```console
  $ scp scripts/board/*.sh xilinx@pynq: && ssh -t xilinx@pynq sudo sh prep.sh
  ```

  It stops the services, removes the kernel modules and unbinds the drivers on PL
  addresses, and says what it could not. A reboot restores the stock desktop.
- for DMA or HLS pointer arguments: load `u-dma-buf` with one buffer per
  `/dev/udmabufN` you plan to use;
- for interrupts: apply the generated `interrupts.dtso` with
  [`scripts/board/overlay.sh`](../reference/scripts.md) (see
  [Interrupts](interrupts.md#one-time-setup-on-the-board)).

### Building u-dma-buf

DMA and HLS pointer arguments need the
[`u-dma-buf`](https://github.com/ikwzm/udmabuf) kernel module, which the image
used for validation did not include. Its headers-only `build` directory cannot
build modules on the board, so cross-build on the host against the same kernel (you need `flex`, `bison`, `make`
and an ARM cross gcc on the host):

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

On the **PYNQ 3.1 image** (kernel `6.6.10-xilinx-v2024.1`, 2026-11-26) the same
happened, and the module was built on the host from a copy of the board's own build
directory, without the kernel source: with `may-sync-config=` on the command line,
`make asm-generic uapi-asm-generic archheaders`; `mach-types.h` and `unistd-nr.h`
from the scripts in `arch/arm/tools`; the host programs `fixdep`, `genksyms` and
`modpost` compiled from `scripts/` (`genksyms` needs the output of `flex` and
`bison`, run on the board here, and `modpost` needs `elfconfig.h` and
`devicetable-offsets.h`); then step 3's module build. The kernel's configuration
files stayed unchanged.

Building and copying binaries is covered in
[Getting started](getting-started.md#build-for-the-board-linux-flavor).

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
matching IP is present). When the design has several instances of a core, pass the
instance name as the first argument (`./gpio_smoke leds_gpio`); `hardroxide inspect`
lists the names, and an unknown name prints the choices:

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
| An interrupt's UIO device is missing after the overlay applied, and `dmesg` shows `genirq: Flags mismatch irq N ... (fabric)` and `probe ... failed with error -16` | the image's own `40000000.fabric` device holds SPI cell 29 (`IRQ_F2P[0]`) | use [`overlay.sh`](../reference/scripts.md), which releases that device when the overlay wants the line |
| `board_check` accepts a line on `IRQ_F2P[0]` but no interrupt can arrive, for an **edge**-triggered core | the image mapped that line as level-triggered at boot, and releasing its device does not change that: the new UIO device appears with no interrupt behind it | move the edge-triggered core to a higher `IRQ_F2P` bit |
| `board_check` says an interrupt is ok although no overlay was applied | a block of the design is at `0x4000_0000`, the address of the image's `fabric` device, which is then found by address; it is the right line only if the block uses `IRQ_F2P[0]` | apply the overlay first |
| `Fpga(PlInUse(..))` | a kernel driver is bound to a PL address; reprogramming under it can freeze the CPU | run `prep.sh` ([setup](#one-time-board-setup)); if it appeared right after power-up, a late service rebound a driver, so run `prep.sh` again once the board has settled |
| `IrqError::ViaController { .. }` | the interrupt goes through an AXI interrupt controller | open the controller's `IRQ_IRQ` and use `AxiIntc::wait` |
| The board freezes during `board_check` step 1 | the PL is in use by another driver (base overlay, HDMI, `zocl`) while it is reprogrammed | stop and unbind them first (One-time board setup); power-cycle to recover |
| The board stops answering while `overlay.sh` replaces one overlay with another | seen once on the PYNQ 3.1 image, after several replacements had worked; both cores ended in the kernel's crash handlers; cause unknown | power-cycle it; applying one overlay per boot avoids the step |
| `reboot` ends with `Reboot failed -- System halted` on the serial console | seen on the PYNQ 3.1 image | shut down, then switch the board off and on |
| The board stops answering (SSH times out, serial silent) | usually reprogramming under a live driver, or an access to an unclocked PL (never read PS-PL HP-port registers unless their clock runs) | power-cycle it; a software `reboot` did not recover a board in this state |
| A DMA or HLS transfer silently loses its last 16 words, or a DMA call fails with `StuckChannel` right after loading, after many design switches | the PS-PL HP port is wedged by a half-finished write burst (see [G17](../08-verification-gaps.md#g17-the-ps-pl-hp-port-can-wedge-after-many-design-switches)) | power-cycle the board |
| `Udmabuf::open` fails | `u-dma-buf` not loaded, or fewer buffers than expected | `ls /dev/udmabuf*` |
| A driver call returns `Timeout` | the IP never signalled completion: wrong clock, unconnected stream, or the core is held in reset | the IP's status registers through `regs()`, and G3 (PL resets) |
