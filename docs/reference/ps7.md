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

The global timer's **comparator interrupt** (PPI 27, `map::ppi::GLOBAL_TIMER`) is a periodic interrupt that does not disturb the kernel tick (which uses the private timer):

```rust,ignore
pub fn start_periodic_interrupt(&self, period: u32) -> Result<()>
pub fn stop_periodic_interrupt(&self) -> Result<()>
pub fn clear_event(&self) -> Result<()>
pub fn event_pending(&self) -> Result<bool>
```

`start_periodic_interrupt(period)` sets the comparator `period` timer ticks ahead and its auto-increment to `period`, so the interrupts come exactly every `period` ticks with no drift (the handler calls `clear_event`; the GIC enables PPI 27 and `irq::register` installs the handler). Under the RTOS the kernel's interrupt path dispatches it.
It drives the audio pump at 97.656 kHz (3328 ticks of 325 MHz).


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

> **Verified on a PYNQ-Z2:** it configured the audio codec over the EMIO-routed I2C 1 (see [Verification gaps](../08-verification-gaps.md)). A bug found there: the `embedded-hal` `transaction` must call the controller's own `write`/`read`, not `self.write` (which resolves to the trait default and recurses).

## SD card controller

```rust,ignore
pub enum SdError
pub struct CardInfo
pub fn clock_divisor(base_hz: u32, target_hz: u32) -> (u32, u32)
pub fn clock_divisor_v3(base_hz: u32, target_hz: u32) -> (u32, u32)
pub fn csd_blocks(r: &[u32; 4]) -> u64
pub struct SdHost<R: Mmio>
pub fn new(regs: R, base_hz: u32) -> Self
pub fn regs(&self) -> &R
pub fn info(&self) -> Option<&CardInfo>
pub fn card_present(&self) -> Result<bool, MmioError>
pub fn init(&mut self) -> Result<CardInfo, SdError>
pub fn init_ignoring_card_detect(&mut self) -> Result<CardInfo, SdError>
pub fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), SdError>
pub fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), SdError>
pub fn sdio_ref_hz(slcr: &impl Mmio, ps_clk_hz: u64) -> Result<u64>
```

`hardroxide_ps7::sd`: the Zynq's SD/SDIO host controller (`map::SDIO0_BASE`, `0xE010_0000`; `SDIO1_BASE` is `0xE010_1000`), an Arasan SDHCI 2.0 with the
standard register set, driven by polling and programmed I/O through the buffer data port (no DMA, no interrupts). `SdHost::new(regs, base_hz)` takes the
mapped registers and the controller's input clock (`clocks::sdio_ref_hz(&slcr, 50_000_000)`, 50 MHz on the PYNQ-Z2). `init` does the whole card
bring-up: it applies the 3.3 V card power, runs the identification at 400 kHz (CMD0, CMD8, ACMD41 with the high-capacity bit, CMD2, CMD3, CMD9, CMD7),
switches the bus to four bits and the clock to 25 MHz, and returns a `CardInfo` (capacity in 512-byte blocks, SDHC or standard capacity, the manufacturer,
the five-character product name and the serial number from the CID, and the bus clock). `init_ignoring_card_detect` skips the card-detect pin, for a board
whose pin is not wired. `read` and `write` move whole 512-byte blocks (any number per call) and wait for the card to leave its programming state after
a write. `SdHost` implements [`BlockDevice`](mmio.md#block-devices), so it plugs straight into [`hardroxide-fat`](fat.md). The card is expected to be a
standard SD card: SDSC, SDHC and SDXC work; MMC and eMMC do not. `SdError::NoResponse` means the card did not answer the first commands (a card that is not
seated, a dead card or one that is not an SD card).

> **Board requirement found on the PYNQ-Z2:** the SD card, the Ethernet PHY and the USB PHY are on **MIO bank 1, which the board supplies at 1.8 V**. The PS
> configuration must say so (`ps7_init` sets `MIO_PIN_40` to `MIO_PIN_45` to `0x1280`, the 1.8 V input/output type); with the 3.3 V setting (`0x1680`) every
> command timed out with a command-conflict error. The generator's board preset carries `PCW_PRESET_BANK0_VOLTAGE` and `PCW_PRESET_BANK1_VOLTAGE` for this.
>
> **Verified on a PYNQ-Z2** with a SanDisk 16 GB and a 32 GB SDHC card (see [Verification gaps](../08-verification-gaps.md)): initialisation at 25 MHz on four bits,
> raw transfers of 1, 4 and 64 blocks, and 4 MiB files at about 1.6 MB/s written and 7.4 MB/s read. A Netac 8 GB microSD did not answer on the board or in a PC's card
> reader, so it was not usable (the card or its adapter). Not run: 1-bit buses, SDSC cards, the second controller, cards that need a voltage switch (UHS).

## Ethernet MAC (`gem`)

```rust,ignore
pub const RX_DESCRIPTORS: usize = 32
pub const TX_DESCRIPTORS: usize = 16
pub const FRAME_BUF: usize = 1536
pub const DMA_REGION_BYTES: usize = TX_BUFS + TX_DESCRIPTORS * FRAME_BUF
pub enum GemError
pub struct Link
pub fn negotiated(advertise_1000: u16, status_1000: u16, advertise: u16, partner: u16) -> Link
pub struct Gem<R: Mmio>
pub unsafe fn new(regs: R, dma_base: usize, mac: [u8; 6]) -> Self
pub fn regs(&self) -> &R
pub fn mac(&self) -> [u8; 6]
pub fn link(&self) -> Option<Link>
pub fn phy_address(&self) -> Option<u8>
pub fn frame_counts(&self) -> Result<(u32, u32), GemError>
pub fn init(&mut self) -> Result<(), GemError>
pub fn mdio_read(&self, phy: u8, reg: u8) -> Result<u16, GemError>
pub fn mdio_write(&self, phy: u8, reg: u8, value: u16) -> Result<(), GemError>
pub fn find_phy(&mut self) -> Result<u8, GemError>
pub fn link_up(&mut self, slcr: &impl Mmio, timeout_ms: u64) -> Result<Link, GemError>
pub fn set_speed(&self, slcr: &impl Mmio, link: Link) -> Result<(), GemError>
pub fn negotiate(&mut self, timeout_ms: u64) -> Result<Link, GemError>
pub fn start(&mut self, link: Link) -> Result<(), GemError>
pub fn mdio_read_extended(&self, phy: u8, reg: u16) -> Result<u16, GemError>
pub fn mdio_write_extended(&self, phy: u8, reg: u16, value: u16) -> Result<(), GemError>
pub fn dp83867_set_delays(&mut self, rx_code: u8, tx_code: u8) -> Result<bool, GemError>
pub fn receive(&mut self, buf: &mut [u8]) -> Option<usize>
pub fn transmit(&mut self, frame: &[u8]) -> bool
```

`hardroxide_ps7::gem`: the Zynq's Gigabit Ethernet MACs (`map::GEM0_BASE`, `0xE000_B000`; `GEM1_BASE` is `0xE000_C000`), a Cadence GEM, in polled mode: descriptor rings
(32 receive, 16 transmit) with a 1536-byte buffer each, whole frames in and out, the management interface (MDIO) and autonegotiation. Enable the crate's `net`
feature and `Gem` implements [`hardroxide_net::Device`](net.md), so the network stack runs on it.

`Gem::new(regs, dma_base, mac)` is `unsafe`: the rings and buffers live in the `DMA_REGION_BYTES` (about 75 KiB) at `dma_base`, which must be 64-byte aligned,
**not cached** (use [`mem::set_non_cacheable`](#memory-mmu-caches-l2-mem) on the section) and not used for anything else, because the MAC's DMA reads and writes it. `init` resets the
MAC, sets up the rings, the station address (one specific-address filter plus broadcast) and the MDIO clock; `link_up(slcr, timeout_ms)` finds the PHY (the first address
that answers), restarts autonegotiation, waits for the link, reads the negotiated speed and duplex (`negotiated` is the pure function of the PHY's registers), sets the MAC's mode and
the transmit clock (`GEM0_CLK_CTRL`: 125, 25 or 2.5 MHz from the 1000 MHz IO PLL) through the SLCR, and turns the receiver and transmitter on. `receive` copies out the next
frame (the FCS is removed by the MAC) and gives the descriptor back; `transmit` queues a frame (the MAC adds the FCS and pads short frames) and returns `false` if every
descriptor is busy. There are no interrupts, no jumbo frames, no VLAN, no checksum offload and no hardware timestamps; the PHY is driven through the standard IEEE registers.

> **Board requirements found on the PYNQ-Z2:** the PS must be configured for Ethernet 0 (`ps7_init` sets MIO 16 to 27 to RGMII, MIO 52 and 53 to the management interface, the MAC clock to
> the IO PLL, and MIO bank 1 to 1.8 V, see [SD card](#sd-card-controller)). The preset script that generates this had a bug: Vivado resets a peripheral's pin group when the peripheral is
> enabled, and the preset's settings are applied one at a time in alphabetical order, so Ethernet 0 came out as EMIO with an external clock and the MAC's pins stayed unconnected (every MDIO
> read returned 0). The preset is now applied in two passes and `fixtures/board/pynq_base_overlay.xsa` was rebuilt; the other board fixtures were not (they have no use for Ethernet yet).
> The PHY is a Realtek RTL8211E that answers at MDIO addresses 0 and 1 (ID `0x001cc915`); it negotiates 1000 Mbit/s full duplex with a PC's port.
>
> **Verified on a PYNQ-Z2** (see [Verification gaps](../08-verification-gaps.md)): link at 1000 Mbit/s full duplex, DHCP from a PC, `ping` at 0.28 ms, the web server at about 3,200 requests/s. Not run: 10 and 100 Mbit/s
> links (the speed paths are written but untested on a board), half duplex, the second MAC, a cable unplugged and replugged while running (the link is not monitored after `link_up`).
