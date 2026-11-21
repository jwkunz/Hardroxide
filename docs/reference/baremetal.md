# Bare-metal crate

`hardroxide generate design.xsa --platform baremetal -o my_design` writes a `no_std` crate for
core 0 of the Zynq-7000 with no operating system, built on [`hardroxide-ps7`](ps7.md)
([ADR 0003](../decisions/0003-bare-metal-and-rtos-platforms.md)). It shares the register layer
(`pac`), the curated drivers and the HLS wrappers with the [Linux crate](generated-crate.md);
the platform glue differs.

```text
Cargo.toml  .cargo/config.toml  build.rs  link.ld  interface.txt  interface.sha256
src/lib.rs  src/pac.rs  src/ips.rs (HLS only)
examples/   board_check  gpio_smoke  timer_delay  timer_irq  bram_test  hls_smoke  dma_loopback  (per IP present)
```

No bitstream is embedded: the PS and the PL must already be configured, by the loader or an
FSBL ([boot flows](../decisions/0003-bare-metal-and-rtos-platforms.md#boot-flows)).
`.cargo/config.toml` selects the `armv7a-none-eabihf` target (build inside the crate directory;
`rustup target add armv7a-none-eabihf`), and `build.rs` links with `link.ld`, rendered from
`hardroxide-ps7`'s template (DDR from 1 MiB, 511 MiB, a stack per processor mode); edit it for
another memory layout. `hardroxide update` leaves a same-interface crate unchanged and, with
`--regen`, keeps `Cargo.toml`, the build files and your `link.ld`.

## `Design`

`Design<B>` has one field per IP, as in the Linux crate. `Design::init()` (ARM only) turns on the MMU and caches ([`ps7::mem`](ps7.md#memory-mmu-caches-l2-mem); `DDR_BYTES` is the DDR size it assumes), starts the
time source the drivers' timeouts use, sets up the console on the PS UART the design enables
(`CONSOLE_UART`; none if the design disables both), programs the GIC with each PL interrupt's
trigger type, and maps every IP. It fails with `PlNotConfigured` rather than let a later register
access hang the CPU. Interrupts stay masked until `enable_interrupts()`. Other items:
`apply_clocks()`, `fclk_hz(index)`, `map_only(backend)` (for host tests), `halt()`, the constants
`PS_CLK_HZ`, `FCLKS`, `CONSOLE_BAUD`, `INTERFACE_HASH` and `IRQ_LINES`, and `ps7` (the PS crate,
re-exported).

A program is `ps7::entry!(main); fn main() -> ! { ... }`, using `ps7::println!` for output. Wait for
a PL interrupt with `ps7::wait::IrqWait::for_line(&pac::<ip>::IRQ_...)` (the `IrqWaiter` the
drivers expect). DMA buffers: `hardroxide_dma::static_dma_buffer!(size)` (non-cacheable; the `dma_loopback` example uses two).

The console needs the PS UART's pins and clocks, which `ps7_init` sets from the design's PS
configuration: a design built without a board preset has UART0 disabled and prints nothing. The
board-accurate fixture scripts in `fixtures/scripts/board/` show how to build one.
