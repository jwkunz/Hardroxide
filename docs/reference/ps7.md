# hardroxide-ps7

Zynq-7000 processing-system support for **bare metal** (no operating system): startup and
exception vectors for the Cortex-A9, the GIC, the MPCore timers, the Cadence UART, a
console, and a time source for the drivers. `no_std`, no third-party dependencies; part of
the bare-metal and RTOS platforms ([ADR 0003](../decisions/0003-bare-metal-and-rtos-platforms.md)).

The PS must already be initialised (DDR, MIO, PLLs) by the vendor's `ps7_init` or an
FSBL. This crate starts a program on core 0 and drives the PS peripherals the program
needs. Everything register-level is generic over [`Mmio`](mmio.md) and unit-tested on the
host; the parts that only make sense on a Cortex-A9 (`start`, `cp15`, `console`, `time`)
exist only for `target_arch = "arm"`, so the crate still builds in a host workspace.

Features: `default-irq-entry` and `default-svc-entry` (on by default: the non-nesting
IRQ vector entry and a diagnostic SVC entry; the RTOS kernel turns them off and supplies
its own) and `panic-handler` (a `#[panic_handler]` that prints the panic and halts).

Verified on a PYNQ-Z2 and under QEMU's `xilinx-zynq-a9` machine
([Verification gaps](../08-verification-gaps.md)): `baremetal/ps7-smoke` prints over the
USB-UART, measures `sleep(100 ms)` at 100.07 ms and takes 20 private-timer interrupts in
200.04 ms.

## Starting a program

```rust,ignore
pub const LINKER_SCRIPT_TEMPLATE: &str
#[macro_export] macro_rules! entry
```

`entry!(path)` declares the program's entry function (`fn() -> !`), run on core 0 in
System mode with interrupts masked. Startup (`_start`, first in the image) parks core 1,
gives every exception mode its own stack, enables the VFP, points `VBAR` at the vector
table, zeroes `.bss` and calls the entry. `LINKER_SCRIPT_TEMPLATE` is the linker script
the program needs (RAM, section order, one stack per mode, a `.dma` section); names
between `@` signs (`@RAM_ORIGIN@`, `@RAM_LENGTH@`, `@STACK_FIQ@` ... `@STACK_SYS@`) are
filled in by the generator or a `build.rs`, as in `baremetal/ps7-smoke`.

An unhandled undefined instruction, prefetch or data abort prints the exception, the
faulting address and the registers (and the fault status registers) on the console and
halts.

## Clocks

```rust,ignore
pub enum Pll
pub fn pll_hz(slcr: &impl Mmio, ps_clk_hz: u64, pll: Pll) -> Result<u64>
pub fn cpu_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64>
pub fn periph_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64>
pub fn uart_ref_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64>
```

Frequencies from the SLCR registers `ps7_init` set (`ps_clk_hz` is 50 MHz on the
PYNQ-Z2). `periph_hz` is half the CPU clock: the clock of the global timer, private
timer and watchdog (325 MHz after the vendor setup, measured on the board).

## UART and console

```rust,ignore
pub struct Uart<R: Mmio>
pub fn baud_setting(ref_clk_hz: u32, baud: u32) -> Option<BaudSetting>
pub fn init(&self, ref_clk_hz: u32, baud: u32) -> Result<Option<u32>>
pub fn try_write_byte(&self, b: u8) -> Result<bool>
pub fn write_byte(&self, b: u8) -> Result<bool>
pub fn try_read_byte(&self) -> Result<Option<u8>>
pub fn set_rx_interrupt(&self, trigger: u32) -> Result<()>
pub unsafe fn init(base: usize, ref_clk_hz: u32, baud: u32) -> Option<u32>
pub fn write_fmt(args: fmt::Arguments<'_>)
```

`Uart::init` programs 8N1 at the requested rate and returns the rate actually produced
(`BAUDGEN`/`BAUDDIV` chosen for the smallest error; 115200 from a 100 MHz reference gives
115207). `Uart` implements `core::fmt::Write`. `console::init` makes a UART the process-wide
console and enables `print!` / `println!` (also used by the exception report and the
panic handler); output from interrupt handlers and the main program can interleave, but
nothing is shared mutably. The UART's pins (MIO 14/15 on the PYNQ-Z2), clock and APB clock
must already be set up: note that a `ps7_init` generated for a design with UART0 disabled
leaves the UART's APB clock (`APER_CLK_CTRL` bit 20) off, and every UART register then
reads zero.

## GIC

```rust,ignore
pub struct Gic<R: Mmio>
pub fn init(&self) -> Result<()>
pub fn enable(&self, id: u32) -> Result<()>
pub fn disable(&self, id: u32) -> Result<()>
pub fn set_priority(&self, id: u32, priority: u8) -> Result<()>
pub fn set_target(&self, id: u32, cpu_mask: u8) -> Result<()>
pub fn set_trigger(&self, id: u32, t: Trigger) -> Result<()>
pub fn send_sgi(&self, sgi: u32, cpu_mask: u8) -> Result<()>
pub fn set_priority_mask(&self, mask: u8) -> Result<()>
pub fn acknowledge(&self) -> Result<Option<u32>>
pub fn end_of_interrupt(&self, id: u32) -> Result<()>
```

The PL390 of the Cortex-A9 MPCore. `init` disables everything, routes shared interrupts
to core 0, sets a middling priority and level triggering, and enables the distributor and
this core's interface. Priorities are numerically lower = more urgent, and only bits 7:3
count. `set_trigger` selects level (`0b01`) or edge (`0b11`) for shared interrupts; the
generated crates set it from the design's interrupt sensitivity, which removes the
level/edge conflicts seen under Linux. `map::f2p_irq(bit)` gives the GIC ID of PL
interrupt `IRQ_F2P[bit]`.

## Interrupt handlers

```rust,ignore
pub type Handler = fn();
pub fn register(id: u32, handler: Handler) -> bool
pub fn unregister(id: u32)
pub fn register_event(id: u32) -> bool
pub fn take_event(id: u32) -> Option<u32>
pub fn dispatch(id: u32) -> Dispatched
pub fn unhandled_count() -> usize
```

A table of plain functions indexed by GIC ID, held in atomics (no locking). The IRQ
vector entry saves the caller-saved registers and VFP state, acknowledges the interrupt,
calls `dispatch`, signals end-of-interrupt and returns. Handlers run on the IRQ stack with
interrupts masked; they may use floating point. `register` does not enable the interrupt
in the GIC.

**Event mode** mirrors a Linux UIO device for the drivers: `register_event(id)` makes the
interrupt count instead of call a function; the vector entry masks it in the GIC when it
fires (so a level-triggered line cannot storm), and `take_event(id)` returns the cumulative
count when it changed. `wait::IrqWait` (ARM only) wraps this as a `hardroxide_irq::IrqWaiter`
(`enable` unmasks, `wait_event` waits for a new event), so `AxiIntc::wait` and the other
drivers work unchanged; waiting with IRQs masked returns an error instead of hanging. An
interrupt with nothing registered is also masked and counted by `unhandled_count`.

## Timers and time source

```rust,ignore
pub struct GlobalTimer<R: Mmio>
pub fn count(&self) -> Result<u64>
pub struct PrivateTimer<R: Mmio>
pub fn start_periodic(&self, ticks: u32) -> Result<()>
pub fn start_one_shot(&self, ticks: u32) -> Result<()>
pub fn clear_interrupt(&self) -> Result<()>
pub fn ticks_to_ns(ticks: u64, hz: u64) -> u64
pub fn duration_to_ticks(d: Duration, hz: u64) -> u64
pub unsafe fn init(periph_hz: u64)
```

The 64-bit global timer is the system time base; `time::init(periph_hz)` starts it and
registers it with [`hardroxide_mmio::time`](mmio.md#time), after which every driver's
timeouts work. The 32-bit private timer (PPI 29) is a down-counter with an optional
periodic interrupt, intended for the kernel tick.

## Testing

```console
$ scripts/test-qemu.sh        # builds baremetal/ps7-smoke, runs it on QEMU, checks the output
```

Needs a QEMU with the `xilinx-zynq-a9` machine (the one bundled with Vitis, or
`qemu-system-arm`). On a board, the same program loads over JTAG after `ps7_init`
([Running on hardware](../guide/hardware.md)).

## Memory: MMU, caches, L2 (`mem`)

```rust,ignore
pub unsafe fn enable(ddr_bytes: usize)
pub fn is_enabled() -> bool
pub unsafe fn set_non_cacheable(base: usize, len: usize)
pub fn clean_range(addr: usize, len: usize)
pub fn invalidate_range(addr: usize, len: usize)
pub fn flush_range(addr: usize, len: usize)
pub const SECTION: usize
```

`enable` installs a flat (virtual = physical) table of 1 MiB sections and turns on the MMU, the L1 instruction and data caches,
branch prediction and the PL310 L2 cache (the generated `Design::init` calls it first). DDR is normal write-back cacheable; the
`.dma` section (what `hardroxide_dma::static_dma_buffer!` uses; the linker template aligns it to 1 MiB) is **normal non-cacheable**, so
the CPU and a PL master see the same memory with no maintenance; everything else (PL, PS peripherals, OCM) is strongly ordered.
Single core: memory is non-shareable and the SCU and `ACTLR.SMP` stay off. For memory a PL master touches outside `.dma`, call
`clean_range` before the PL reads it and `invalidate_range` before the CPU reads what the PL wrote (32-byte-aligned ranges);
`flush_range` does both. `set_non_cacheable(base, len)` is the other route: it makes the DDR sections covering a fixed physical range non-cacheable (after writing back and
discarding anything cached there; the range is rounded out to 1 MiB sections), for a buffer a PL master uses at a fixed address outside `.dma`, such as a frame buffer in the
window a video DMA can reach. On the PYNQ-Z2 a loop over 64 KiB ran 23 times faster with the caches on (100.7 ms against 4.4 ms for 20 passes), and a DMA loopback over
`.dma` buffers passes with them on.

## I2C controllers

```rust,ignore
pub const FIFO_DEPTH: usize = 16;
pub enum I2cError
pub fn clock_divisors(input_hz: u32, scl_hz: u32) -> Option<(u32, u32, u32)>
pub struct I2c<R: Mmio>
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn set_clock(&self, input_hz: u32, scl_hz: u32) -> Result<u32, I2cError>
pub fn write(&self, addr: u8, data: &[u8]) -> Result<(), I2cError>
pub fn read(&self, addr: u8, buf: &mut [u8]) -> Result<(), I2cError>
pub fn write_read(&self, addr: u8, out: &[u8], buf: &mut [u8]) -> Result<(), I2cError>
pub fn cpu_1x_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64>
```

`hardroxide_ps7::i2c`: the Cadence I2C controllers `I2C0` (`map::I2C0_BASE`, `0xE000_4000`) and `I2C1` (`0xE000_5000`), master mode, polled,
7-bit addresses, as the vendor driver's polled master functions drive them. `I2c` also implements `embedded_hal::i2c::I2c`. On the PYNQ-Z2's base
overlay `I2C1` is routed through the EMIO to the audio codec's control pins. `set_clock(input_hz, scl_hz)` sets the bus clock from the controller's
input clock (`clocks::cpu_1x_hz`, the CPU clock divided by 6 in the 6:2:1 mode) using the vendor's divisor search
(`f = input / (22 * (div_a + 1) * (div_b + 1))`, 100 kHz targets lowered to 90 kHz, above 384.6 kHz limited) and returns the clock produced.
A transfer is at most one FIFO (16 bytes; longer is `I2cError::TooLong`), `write_read` is two transfers with a stop between (not a repeated start), and a
missing acknowledge is `I2cError::Nack(address)`.

> **Not yet run on a board** (see [Verification gaps](../08-verification-gaps.md)): the register sequence and the divisor search are unit-tested.
