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
