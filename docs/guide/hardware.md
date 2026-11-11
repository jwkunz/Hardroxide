# Running on hardware

No driver has been validated on a board yet. Before relying on anything, work
through [Verification gaps](../08-verification-gaps.md) **in order**: later
checks assume the earlier ones passed.

## One-time board setup

On the stock PYNQ-Z2 image, as root:

- the FPGA manager must exist: `ls /sys/class/fpga_manager/fpga0`;
- `/dev/mem` must be usable by root (it is on the PYNQ image);
- for DMA or HLS pointer arguments: load `u-dma-buf` with one buffer per
  `/dev/udmabufN` you plan to use;
- for interrupts: apply the generated `interrupts.dtso` (see
  [Interrupts](interrupts.md#one-time-setup-on-the-board)).

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
| `IrqError::ViaController { .. }` | the interrupt goes through an AXI interrupt controller | open the controller's `IRQ_IRQ` and use `AxiIntc::wait` |
| `Udmabuf::open` fails | `u-dma-buf` not loaded, or fewer buffers than expected | `ls /dev/udmabuf*` |
| A driver call returns `Timeout` | the IP never signalled completion: wrong clock, unconnected stream, or the core is held in reset | the IP's status registers through `regs()`, and G3 (PL resets) |
