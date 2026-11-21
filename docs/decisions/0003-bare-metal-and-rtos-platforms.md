# ADR 0003: Bare-metal and RTOS platforms

Status: **proposed** (decisions below are the maintainer's; the plan is a draft
for review). Supersedes, for the new platforms only, the scope rule "PL only;
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
   repository. See "Dependencies" below for what that does and does not mean.

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

What stays: the existing optional `embedded-hal` trait impls in `hardroxide-ip`
(traits only; they become a feature, off for the new crates) and `thiserror`
(compile-time derive, no runtime code, used with `default-features = false`).
The Vitis tools are an external requirement of the host commands, not a Rust
dependency.

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
| 0 | Recon (this ADR's findings): tools, JTAG, QEMU, targets | in progress |
| 1 | `no_std` refactor of the shared crates (Linux unchanged) | all existing tests and the armv7 emulation pass |
| 2 | `hardroxide-ps7`: startup, UART, timers, GIC; QEMU tests; then on the board via JTAG | hello on the UART; a PL interrupt serviced; time source accurate |
| 3 | `--platform baremetal`, `hardroxide run`, board-accurate PS7 fixtures | `fixtures/board_tests` programs run on the board |
| 4 | MMU, caches, L2, DMA buffers; jitter benchmark | measured latency and jitter recorded |
| 5 | `hardroxide-rt` and `--platform rtos` | priority-inheritance and stress tests; 10 ms task jitter measured over hours |
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
  JTAG and UART. `hw_server` cannot use it until the udev rule below is installed.
- **The test fixtures' PS7 configuration is not board-accurate.** 300 of about 900
  `PCW_` settings differ from the board's base overlay: no MIO pin-mux (178
  settings), UART0 on MIO 14..15 disabled, SD/QSPI/Ethernet/USB disabled, DDR at
  the stock 533 MHz instead of the board's 525 MHz custom timing. That did not
  matter under Linux (U-Boot initialises the PS); a `ps7_init` from a fixture would
  give no console and unverified DDR. Bare-metal fixtures must copy the PS7
  settings from the PYNQ base overlay's HWH (Apache-2.0).
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
  (`0403`) world-writable, and it unbinds `ftdi_sio` from **both** FT2232
  interfaces, which removes `/dev/ttyUSB0` and `/dev/ttyUSB1`. The console UART
  (interface 1) then has to be read another way (the host loader provides it) or
  `ftdi_sio` rebound to that interface. A narrower rule that releases only the
  JTAG interface is preferable once tested.
- Where the Vitis tools are looked for, what `hardroxide doctor` checks, and what
  the commands do when something is missing (warn and halt).
- Putting the PYNQ-Z2 into JTAG boot mode (and back to SD for Linux).
- A reminder that running a bare-metal program takes over the board: Linux and
  `ssh` are gone until the next SD boot.
