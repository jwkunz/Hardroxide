# ADR 0003: Bare-metal and RTOS platforms

Status: **accepted** 2026-11-20 (the decisions below are the maintainer's; the phase
plan may be revised as work proceeds). Supersedes, for the new platforms only, the scope rule "PL only;
Linux owns PS peripherals".

## Decision

Hardroxide generates one of three kinds of crate, chosen by the user
(`hardroxide generate --platform linux|baremetal|rtos`):

| Platform | What it is |
|---|---|
| `linux` | today's crate: Linux userspace on the PS (unchanged, and the default) |
| `baremetal` | a minimal `no_std` crate that runs on the Cortex-A9 with no operating system |
| `rtos` | `baremetal` plus a FreeRTOS-style real-time kernel written for this board, with generated scaffolding (tasks, tick, console) |

Host-side tools load the FPGA and the executable over JTAG (and build a bootable
`BOOT.BIN`), so a bare-metal program is a `cargo build` away from running.

This is a **major version increment** (2.0): `--platform` becomes a central CLI
concept, and the runtime crates gain a `std`/`no_std` split.

Fixed by the maintainer:

1. **Goal: real-time control**, with **10 ms control loops**. The design
   pressure is on determinism and jitter, not raw speed: a 650 MHz A9 has ample
   headroom for 10 ms loops and a 1 kHz tick.
2. **Preemptive, fixed-priority threads** (FreeRTOS-style) are the kernel's
   primary model. An async executor may later run as one task.
3. **Single core first** (core 0). Core 1 stays parked by the boot ROM.
4. **Host tools are the Vitis tools** (`xsdb`, `hw_server`, `bootgen`, the
   `ps7_init` shipped in the XSA). If they are missing, the host commands warn
   and **halt** with a clear message; nothing is silently degraded.
5. **No third-party dependencies for the new platform code.** The startup code,
   MMU/cache/GIC/timer/UART/PCAP support, and the kernel are written in this
   repository. The existing `embedded-hal` (traits only) and `thiserror`
   (compile-time derive) dependencies are accepted and stay. See "Dependencies"
   below.

## Motivation

Linux is the right home for most PL work, but a user with a hard real-time
control loop wants no OS jitter, no scheduler they do not control, and the
ability to boot straight into their program. A FreeRTOS-like environment in Rust
gives that without leaving the language, and the generator already knows every
address, register, interrupt and clock of the design.

## What Linux provides today, and its replacement

| The Linux crate uses | Bare metal needs |
|---|---|
| `fpga_manager` to program the PL | the boot flow programs it (default), or a PCAP driver |
| `/dev/mem` mmap | plain volatile access to physical addresses (`PhysMem` backend) |
| UIO + device-tree overlay for interrupts | GIC driver, vector table, IRQ handlers (trigger type from the HWH) |
| `u-dma-buf` for DMA memory | linker-placed buffers in a known memory attribute |
| `std` (`Instant`, `File`, `Vec`, `io::Write`) | `no_std`, a time source, optional `alloc` |
| the kernel's startup, MMU, stacks | startup code, linker script, `armv7a-none-eabihf` |
| `scp` + `ssh` to run | JTAG load, or an SD-card `BOOT.BIN` |

## Design

### Crates

```text
hardroxide-hwh, hardroxide-gen, hardroxide-cli     host only (std)
hardroxide-mmio, -ip, -irq(trait), -fpga(slcr)     shared; gain a `std` feature, otherwise no_std
hardroxide-ps7        NEW  Zynq-7000 PS support, no_std: startup, vectors, MMU, caches, GIC,
                           timers, UART console, SLCR, PCAP
hardroxide-rt         NEW  the kernel: scheduler, tasks, sync, time, ISR hand-off
```

`pac`, the drivers and the generator model are common to all three platforms.

### Existing crates become `no_std`-capable

- A default-on `std` feature keeps the Linux crate and every existing test
  unchanged. Without it the crates are `#![no_std]`.
- **Timeouts.** Drivers call `Instant::now()` internally. A small time-source
  trait with a registered global (`hardroxide_mmio::time`) replaces it, so no
  public signature changes (`Duration` is in `core`). Linux registers
  `std::time::Instant`; bare metal registers a hardware timer.
- `AxiFifo` uses `Vec`: an `alloc`-free `recv_into` stays; `Vec` APIs sit behind
  `alloc`. `AxiUart`'s `io::Write` impl gets a `core::fmt::Write` sibling.
- `IrqWaiter` is already a trait, so `AxiIntc::wait` works unchanged once a GIC
  based waiter exists. `DmaBuffer` is already abstract.

### `hardroxide-ps7`

All `unsafe` hardware access of the bare-metal side is confined here and in the
kernel's context-switch code, documented as in the rest of the project.

- **Startup**: vector table, per-mode stacks (SVC, IRQ, FIQ, ABT, UND, SYS),
  `.bss` clear, VFP/NEON enable, call `main`. Linker script generated from the
  design's memory map.
- **CP15**: SCTLR, TTBR, DACR, cache and TLB maintenance, MPIDR wrappers.
- **MMU**: a static 1 MiB-section L1 table. Defaults: DDR normal write-back
  cacheable; PL window and peripherals device/strongly ordered; OCM as needed; a
  non-cacheable region for DMA buffers.
- **Caches**: L1 I/D and the PL310 L2 controller.
- **GIC (PL390)**: distributor and CPU interface, priorities, trigger types from
  the HWH, SGI, acknowledge/end-of-interrupt, and the priority mask used by the
  kernel's critical sections.
- **Timers**: the global timer (64-bit time source) and the private timer
  (kernel tick).
- **UART**: Cadence UART console (the PYNQ-Z2's is UART0 on MIO 14..15); the
  console UART is read from the HWH, and a design with none enabled gets a
  generator warning.
- **PCAP** (devcfg): program the PL from the embedded `design.bin` (optional
  mode; see below).
- **Not in scope: DDR and MIO initialisation.** Reimplementing it from the PS7
  parameters is large and risky. The PS is initialised by the vendor's
  `ps7_init` (JTAG flow, run by `xsdb` from the user's XSA) or by an FSBL (SD
  flow). See "Licensing".

### `hardroxide-rt` (the kernel)

FreeRTOS-like: fixed-priority preemptive scheduler, static allocation by
default, tasks with their own stacks, `delay` and `delay_until` (periodic
release), mutexes with priority inheritance, binary and counting semaphores,
bounded queues, task notifications / event flags, software timers, ISR-safe
calls and a deferral path from interrupts to tasks, an idle task, stack
high-water marking. Cortex-A9 specifics: the context switch is a software
interrupt (the A9 has no PendSV), critical sections raise the GIC priority mask
so high-priority interrupts are not blocked, and VFP/NEON state is saved only
for tasks declared as using floating point.

The scheduling policy (ready lists, priority inheritance, timer wheel) is plain
Rust with no hardware access, so it is unit-tested on the host; only the context
switch and tick are architecture-specific.

A Rust-specific benefit over C FreeRTOS: the generated `Design` can be split so
each IP is moved into the task that owns it, and the compiler stops two tasks
driving the same IP (needs a `split()` on `Design`, which today holds a
singleton guard).

### Generated crates

- **baremetal**: no_std `Design` with a `PhysMem` backend, `Design::init()` (the
  PL is already configured; an optional `Design::program_pl()` uses PCAP and the
  embedded bitstream), the interrupt table as constants (GIC number and trigger
  type from the HWH) instead of `interrupts.dtso`, static DMA buffers, a
  generated linker script, and examples that print on the UART.
- **rtos**: baremetal plus `hardroxide-rt`, a generated `main` with task
  scaffolding and per-task resource ownership.

### Host side (`hardroxide`, new subcommands)

- `hardroxide run <elf> --xsa design.xsa`: starts `hw_server`, runs the XSA's
  `ps7_init.tcl` after a system reset, programs the PL, downloads the ELF,
  starts it and streams the UART console. Implemented by generating `xsdb` Tcl
  and invoking Vitis; the loader sits behind a trait so another backend can be
  added.
- `hardroxide package`: emits a `.bif` and runs `bootgen` for an SD-card
  `BOOT.BIN` (FSBL + bitstream + ELF).
- `hardroxide doctor`: checks that the Vitis tools are found, that the JTAG cable
  is accessible (see "Host setup"), and reports what is missing. `run` and
  `package` run the same checks first and stop with the same message.

## Boot flows

| Flow | PS init | PL programmed by | Use |
|---|---|---|---|
| **A (first)** JTAG | `ps7_init.tcl` from the XSA, run by `xsdb` | `xsdb` | development |
| **B** SD card | FSBL | FSBL (from `BOOT.BIN`) | standalone deployment |
| **C** (later) | either | the app, through PCAP | reprogram without a host |

## Dependencies

"No dependencies" means: no third-party crates for the platform layer
(startup, PS drivers, kernel), and no code copied in from other projects. The
new crates depend only on `core` and on other `hardroxide-*` crates.

**Accepted dependencies** (maintainer's decision): `embedded-hal` 1.0 (traits only,
used by the driver impls in `hardroxide-ip`) and `thiserror` (a compile-time
derive with no runtime code; `no_std` with `default-features = false`). Nothing
else is added. The Vitis tools are an external requirement of the host commands,
not a Rust dependency.

**References we may read.** The authoritative sources for register behaviour are
available locally and are used as *references*, not copied:

- the Xilinx `embeddedsw` sources shipped with Vitis (`~/AMD/2025.2/data/embeddedsw`:
  `standalone`, `uartps`, `scugic`, `devcfg`, `ttcps`, `zynq_fsbl`), licensed MIT;
- Vivado's PS7 register database (`.../zynqconfig/2.0/ps7regs/sw_regs.xml`: 400 KB
  of addresses, fields and descriptions for SLCR, devcfg, UART, TTC, GPIO,
  L2 cache, MPCore, AFI and more); its licence has not been checked, so it is
  used for cross-checking only until it is;
- the Zynq-7000 TRM (UG585), the Cortex-A9 and PL310/GIC documentation.

Each register block gets a unit test against the register database, in the same
spirit as the existing driver tests against the HWH.

## Licensing

- `ps7_init.tcl` is plain register-write data with no licence header; the
  `ps7_init_gpl.c` variant in the XSA is GPLv2 or later. The host loader runs the
  **user's own** `ps7_init.tcl` at run time and redistributes nothing; generated
  crates never embed those tables.
- A legal reading is still needed before any generated artifact embeds vendor
  data. Until then none does.

## Verification plan

1. **Host unit tests** for everything without hardware access (scheduler,
   register-block layout against the register database, generator output, golden
   snapshots).
2. **QEMU** (the Vitis-bundled `qemu-system-aarch64 -M xilinx-zynq-a9`, verified
   to run a bare-metal Rust program and print on the UART): startup, MMU, GIC,
   timers, UART, context switch, in CI. The PL is not modelled; PL accesses are
   mocked.
3. **On the board** over JTAG with the existing fixtures and `fixtures/board_tests`
   programs rebuilt for bare metal; compare outputs with the Linux runs.
4. **Real-time acceptance**: a PL timer interrupt where the ISR reads the
   free-running counter measures interrupt latency and jitter; a 10 ms periodic
   task's release jitter is logged over a long run, with a histogram. Targets are
   set once the first numbers exist.
5. **G17**: with no kernel involved, bare metal is a clean way to learn whether
   the HP-port wedge seen under Linux comes from the PL reprogramming sequence.

## Phases

| Phase | Deliverable | Gate |
|---|---|---|
| 0 | Recon: tools, JTAG, QEMU, targets, console, DDR | **complete** (2026-11-20; findings below) |
| 1 | `no_std` refactor of the shared crates (Linux unchanged) | all existing tests and the armv7 emulation pass; **complete** |
| 2 | `hardroxide-ps7`: startup, UART, timers, GIC; QEMU tests; then on the board via JTAG | hello on the UART; a PL interrupt serviced; time source accurate; **complete** |
| 3 | `--platform baremetal`, `hardroxide run`, board-accurate PS7 fixtures | `fixtures/board_tests` programs run on the board; **complete** for the GPIO, timer, interrupt and HLS paths |
| 4 | MMU, caches, L2, DMA buffers; jitter benchmark | measured latency and jitter recorded |
| 5 | `hardroxide-rt` and `--platform rtos` | priority-inheritance and stress tests; 10 ms task jitter measured over hours; **kernel, generator platform and smoke tests done (2026-11-21); hours-long jitter run outstanding** |
| 6 | `hardroxide package` (BOOT.BIN), tutorials and reference pages, 2.0.0 | standalone boot of an rtos crate |

Later: bare metal on core 1 beside Linux on core 0 (shared GIC, SLCR and memory
make this a separate project).

## Findings from the recon

- Vitis 2025.2 tools are installed under `~/AMD/2025.2` (`xsdb`, `hw_server`,
  `bootgen`, `program_flash`), not on `PATH`; `openocd` is not installed.
- `armv7a-none-eabihf` is prebuilt for stable Rust; a `no_std` program with
  startup assembly builds and links with no extra setup.
- The bundled QEMU has `xilinx-zynq-a9`; a bare-metal Rust program printed on both
  UARTs (the model needs the PS PLL and UART clock and reset set up first, as
  `ps7_init` does on hardware).
- The PYNQ-Z2's FT2232 (`0403:6010`, "Xilinx TUL", serial `1234-tul`) provides
  JTAG (interface 0) and the UART (interface 1, `/dev/ttyUSB1`). `hw_server` could
  not use it until the vendor's udev rule was installed and the cable re-plugged.
  **Verified on the board** after that: `hw_server` and `xsdb` enumerate the
  chain (`arm_dap`, `xc7z020`, both Cortex-A9 cores), and the loop below works.
  After the rule, `/dev/ttyUSB0` and `/dev/ttyUSB1` still exist (mode 0666) and
  `ftdi_sio` is still bound, so the rule's unbind did not take effect here;
  `hw_server` works regardless.
- **The JTAG development loop works with no power cycle**: `rst -system`, `stop`,
  `dow <elf>` (to on-chip memory, no DDR needed), `con`, `mrd`. Every
  `rst -system` recovered the board. Results read back over JTAG matched the
  program's own prints (`MIDR 0x413FC090` = Cortex-A9 r3p0, `PSS_IDCODE 0x23727093`
  = xc7z020, MMU/caches off, System mode, `BOOT_MODE` 0 = JTAG, `REBOOT_STATUS`
  changes on reset).
- **JTAG boot mode does not bypass the PLLs** (`BOOT_MODE[4]` = 0): at reset the
  ARM and IO PLLs run at the default feedback of 26 (1300 MHz), unlike the vendor
  design (IO PLL 1000 MHz). A program must read the PLL setup to derive a UART
  divider, not assume one. After the fixture's `ps7_init`: ARM clock 650 MHz
  (`ARM_CLK_CTRL` divisor 2), global timer 325 MHz (measured: about 1.977 billion
  ticks in a 6 s test).
- **Console works**: UART0 on MIO 14 (RX) / 15 (TX), read on `/dev/ttyUSB1` at
  115200. It needs `MIO_PIN_14 = 0x16E1` and `MIO_PIN_15 = 0x16E0` (3.3 V, pull-up,
  `L3_SEL` = 7; values derived from the base overlay's HWH), a 100 MHz UART clock
  (`UART_CLK_CTRL`, divisor `IO_PLL/100 MHz`), `BAUDGEN` 124 and `BAUDDIV` 6, and
  **the UART0 APB clock enabled** (`APER_CLK_CTRL` bit 20, `UART0_CPU_1XCLKACT`).
  That last bit is cleared by the fixture's `ps7_init` (UART0 is disabled in the
  fixture) and then every UART register reads zero; a board-accurate `ps7_init`
  sets it. Programs must not assume it.
- **The fixture's `ps7_init` brings DDR up on the real board**: after it,
  `DDRC_CTRL` reads `0x85` and a 16 MiB, two-pass pattern test from a program
  running in DDR (MMU and caches off, about 6 s) found 0 errors. So DDR does not
  need the board-accurate configuration to work; MIO, UART and the other
  peripherals still do (see below).
- **Serial capture pitfall**: only one process may read `/dev/ttyUSB1`. Stale
  `cat` readers split the byte stream and produce missing or reordered lines.
- Register-map slips to avoid: `REBOOT_STATUS` is at `0xF8000258` and `BOOT_MODE`
  at `0xF800025C`.
- **The test fixtures' PS7 configuration is not board-accurate.** 300 of about 900
  `PCW_` settings differ from the board's base overlay: no MIO pin-mux (178
  settings), UART0 on MIO 14..15 disabled, SD/QSPI/Ethernet/USB disabled, DDR
  timing parameters differ (the same 525 MHz clock results). That did not matter
  under Linux (U-Boot initialises the PS). On the board, DDR works with a fixture's
  `ps7_init` (above), but the console needs the extra setup above, and a design
  that must use Ethernet, SD, USB or QSPI needs the real settings. Bare-metal
  fixtures should copy the PS7 settings from the PYNQ base overlay's HWH
  (Apache-2.0).
- A crates.io ecosystem for Zynq-7000 exists (a PAC, runtime and HAL under
  `zynq7000-rs`; a pure-Rust FreeRTOS translation, `freertos-in-rust`, whose
  Cortex-A9 port targets QEMU's `vexpress-a9`). It was surveyed and **not
  adopted**, by decision 5; it remains a useful second opinion when a hardware
  behaviour is in doubt.

## Documentation obligations

The eventual documentation must include, as a **Host setup** page:

- **The JTAG cable's udev rule.** `hw_server` needs write access to the FT2232
  and the kernel's `ftdi_sio` driver released. Install Vitis's rule once, as
  root:

  ```console
  $ sudo cp ~/AMD/2025.2/data/xicom/cable_drivers/lin64/install_script/install_drivers/52-xilinx-ftdi-usb.rules /etc/udev/rules.d/
  $ sudo udevadm control --reload-rules && sudo udevadm trigger
  ```

  then unplug and replug the board's USB cable (a rule only applies when the
  device is added). Caveats to state: the rule makes **every** FTDI device
  (`0403`) world-writable (`MODE:="666"`), and as written it asks udev to unbind
  `ftdi_sio` from **both** FT2232 interfaces. On the validated host the unbind did
  not happen (both `ttyUSB` nodes remained and `hw_server` still worked), but
  another host may behave differently; if `/dev/ttyUSB1` disappears, rebind it
  (`echo -n <bus-id>:1.1 > /sys/bus/usb/drivers/ftdi_sio/bind`, as root). A
  narrower rule that grants access to this one cable only is preferable once
  tested.
- Reading the console: use **one** reader on `/dev/ttyUSB1` (115200 8N1); a second
  reader splits the stream. `hardroxide run` provides the reader so users do not
  need `cat`.
- Where the Vitis tools are looked for, what `hardroxide doctor` checks, and what
  the commands do when something is missing (warn and halt).
- Putting the PYNQ-Z2 into JTAG boot mode (and back to SD for Linux).
- A reminder that running a bare-metal program takes over the board: Linux and
  `ssh` are gone until the next SD boot.
