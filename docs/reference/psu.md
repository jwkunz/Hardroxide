# hardroxide-psu

Zynq UltraScale+ processing-system support for **bare metal** on the Cortex-A53: startup and exception vectors, the MMU and
caches, the GIC, the generic timer, clocks, the Cadence UART and a console. `no_std`, no third-party dependencies. It is the
counterpart of [`hardroxide-ps7`](ps7.md) for the Zynq-7000, new in 3.0 ([ADR 0005](../decisions/0005-ultrascale-plus.md)), which
is in progress: the generator and the kernel do not use it yet.

The PS must already be initialised (DDR, MIO, PLLs) by the vendor's `psu_init` or an FSBL. A program runs **on core 0, in
AArch64, at EL3**, the state of the processor after a reset over JTAG (what [`hardroxide run`](cli.md#run) does); started any
other way it sleeps. The module names are those of `hardroxide-ps7` where the hardware allows it, and the drivers for blocks
both families have (`uart`, `i2c`) are that crate's, re-exported. `ps7` has a `cpu` module (an alias of the common part of
its `cp15`) so that a program can name the processor operations the same way on both.

Features: `default-irq-entry` and `default-sync-entry` (on by default: the non-nesting IRQ vector entry and a diagnostic entry
for synchronous exceptions; a kernel supplies its own) and `panic-handler`.

Verified on an RFSoC 4x2 (`scripts/test-board-rfsoc.sh`) and under QEMU's `xlnx-zcu102` machine
(`scripts/test-qemu-psu.sh`): `baremetal/psu-smoke` measures `sleep(100 ms)` at 100.006 ms, takes 20 timer interrupts in
200.17 ms, tests 64 MiB at each end of both DDR ranges, and with the loopback fixture takes interrupts from the PL.

## Starting a program

```rust,ignore
pub const LINKER_SCRIPT_TEMPLATE: &str
#[macro_export] macro_rules! entry
```

`entry!(path)` declares the program's entry function (`fn() -> !`), run with interrupts masked. Startup (`_start`, first in the
image) parks the other cores, takes IRQ, FIQ and system errors to EL3, enables floating point, sets the Cortex-A53's
coherency bit, points `VBAR_EL3` at the vector table, gives exceptions their own stack, zeroes `.bss` and calls the entry.
The linker script needs RAM in DDR below 2 GiB (`@RAM_ORIGIN@`, `@RAM_LENGTH@`) and two stack sizes (`@STACK_EXCEPTION@`,
`@STACK_MAIN@`), as in `baremetal/psu-smoke/build.rs`. Build for `aarch64-unknown-none`.

An unhandled exception prints its kind, the return address, the syndrome register (and the fault address for memory faults) and
the registers, and halts.

## Clocks (`clocks`)

```rust,ignore
pub enum Pll
pub fn pll_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, pll: Pll) -> Result<u64>
pub fn cpu_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64) -> Result<u64>
pub fn uart_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64>
pub fn sdio_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64>
pub fn i2c_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64>
pub fn timestamp_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64) -> Result<u64>
```

Read from the clock registers of both power domains (`map::CRL_APB_BASE`, `map::CRF_APB_BASE`), given the board's reference
(33.333 MHz on the RFSoC 4x2).

## Time and timers (`time`, `timer`)

```rust,ignore
pub unsafe fn init(counter_hz: u64)
pub fn start_periodic(ticks: u64)
pub fn start_rate(counter_hz: u64, rate_hz: u64)
pub fn next_period(counter_hz: u64, rate_hz: u64, carried: &mut u64) -> u64
pub fn start_one_shot(ticks: u64)
pub fn tick()
pub fn stop()
pub fn counter() -> u64
pub fn duration_to_ticks(d: Duration, hz: u64) -> u64
pub fn ticks_to_ns(ticks: u64, hz: u64) -> u64
```

`time::init` takes the system counter's frequency (`clocks::timestamp_hz`; `psu_init` and the boot loader start the counter)
and becomes the time source of the drivers. The timer is the core's secure physical timer, interrupt
`map::ppi::SECURE_PHYSICAL_TIMER`; its handler calls `timer::tick`, which keeps a periodic timer free of drift.
`start_rate` fires a given number of times a second exactly on average, also when that does not divide the counter's frequency
(1 kHz from the RFSoC 4x2's 33,333,333 Hz): the periods differ by one count.

## Interrupts (`gic`, `irq`, `wait`, `cpu`)

`gic::Gic`, `irq::register`, `irq::register_event`, `irq::take_event` and `wait::IrqWait` are the interfaces of
`hardroxide-ps7`, over the 192 interrupt IDs of this family. PL interrupt `bit` (0..8 `pl_ps_irq0`, 8..16 `pl_ps_irq1`) is
`map::pl_irq(bit)`: IDs 121..=128 and 136..=143.

```rust,ignore
pub const fn pl_irq(bit: u32) -> u32
pub fn line_status(&self, id: u32) -> Result<bool>
pub unsafe fn enable_irq()
pub fn disable_irq()
pub fn without_irq<T>(f: impl FnOnce() -> T) -> T
pub fn current_el() -> u32
```

## I2C (`i2c`)

`hardroxide-ps7`'s [driver](ps7.md), for the controllers at `map::I2C0_BASE` and `I2C1_BASE`, with their input clock
`clocks::i2c_ref_hz`: `i2c.set_clock(input_hz, 100_000)`. On the RFSoC 4x2 bus 0 carries nine INA220 power monitors
(`0x40`..`0x48`) and bus 1 a voltage-setting DAC (`0x0d`) and the identity EEPROM (`0x50`, `0x58`);
`baremetal/i2c-info` reads them without changing anything. QEMU's `xlnx-zcu102` does not model these controllers.

## SD card (`sd`)

`hardroxide-ps7`'s [driver](ps7.md), for the controllers at `map::SDIO0_BASE` and `SDIO1_BASE`, given
`clocks::sdio_ref_hz`. These controllers are of specification 3.00, which the driver detects and divides the clock for
(`sd::clock_divisor_v3`: a power of two would clock identification at 732 kHz from the RFSoC 4x2's 187.5 MHz).

```rust,ignore
pub fn clock_divisor_v3(base_hz: u32, target_hz: u32) -> (u32, u32)
```

## Ethernet (`gem`, feature `net` for the network stack)

`hardroxide-ps7`'s [driver](ps7.md), for the MACs at `map::GEM0_BASE`..`GEM3_BASE`; it recognises the later controller from its
identification register (a 64-bit bus; the unused second queue is parked) and refuses rings or buffers above 4 GiB. The
transmit clock is set in `CRL_APB`:

```rust,ignore
pub fn link_up<R: Mmio>(
pub fn reference_hz(speed_mbps: u16) -> u64
pub fn gem_ref_hz(crl: &impl Mmio, crf: &impl Mmio, ps_ref_hz: u64, n: usize) -> Result<u64>
pub fn set_gem_ref_hz(
pub fn divisors(source_hz: u64, target_hz: u64) -> (u32, u32)
```

`gem::link_up(&mut gem, &crl, &crf, ps_ref_hz, instance, timeout_ms)` negotiates, sets the clock and starts the MAC. The
RFSoC 4x2's cable is on `GEM1`, and its TI DP83867 PHY first needs the board's clock delays:
`gem.dp83867_set_delays(0x8, 0xA)`.

## Memory (`mem`)

```rust,ignore
pub unsafe fn enable(ddr_bytes: usize)
pub unsafe fn set_non_cacheable(base: usize, len: usize)
pub fn clean_range
pub fn invalidate_range
pub fn flush_range
```

A flat mapping: DDR cacheable (below 2 GiB, and above it at 32 GiB), the `.dma` section non-cacheable on whole 2 MiB blocks
(`mem::SECTION`), everything else device memory. The last three take `(addr: usize, len: usize)`.
