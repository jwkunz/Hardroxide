# Changelog

## Unreleased

## 2.1.0

The PYNQ base overlay on bare metal and under the kernel ([ADR 0004](docs/decisions/0004-base-overlay-applications.md)).

### New
- `fixtures/board/pynq_base_overlay.xsa`: the PYNQ base overlay with a `ps7_init` (built from the PYNQ-Z2 board preset by
  `fixtures/scripts/build_pynq_base_ps7.tcl` and `package_base_overlay.py`), so `hardroxide run` starts it.
- `hardroxide-rt`: `IrqEvent`, a task sleeps on a hardware interrupt (`new`, `for_line`, `wait`, `rearm`); the kernel's
  interrupt path wakes it, with no handler to write.
- `hardroxide-ip`: AXI GPIO interrupt registers (`enable_interrupt`, `disable_interrupt`, `interrupt_pending`, `clear_interrupt`);
  AXI interrupt controller simulation mode (`start_simulation`, `simulate`).
- `apps/base/`: four programs for the base overlay: a self-test, buttons through the interrupt controller, an RTOS mood light
  with PWM and an interrupt-to-task latency benchmark (4.2 us mean, 6.2 us worst over 2000 interrupts under load).
- `scripts/test-board.sh`: a board regression that generates, builds and runs the self-checking programs over JTAG.
- Application tutorials: a new group, with "Lights and buttons on the base overlay"; a reference page for the base overlay.

### Fixed
- Tests: each generated bare-metal crate builds in its own target directory (same-named crates collided in a shared one).

## 2.0.0

Bare-metal and RTOS platforms ([ADR 0003](docs/decisions/0003-bare-metal-and-rtos-platforms.md)).
Linux crates are unchanged; their dependencies move to 2.

### New
- `hardroxide generate --platform baremetal|rtos`: a `no_std` crate for core 0 of the
  Zynq-7000 with no operating system, built on the new `hardroxide-ps7` (start-up, exception
  vectors, GIC, timers, UART console, MMU, L1/L2 caches with a non-cacheable `.dma` section,
  cache maintenance), and, for `rtos`, on the new `hardroxide-rt` kernel (priority-preemptive
  tasks, delays, periodic release, semaphores, priority-inheriting mutexes, queues).
- `hardroxide run`: load a program over JTAG with the design's own `ps7_init` and show the console.
- `hardroxide package`: build a BOOT.BIN (vendor FSBL, bitstream, program).
- `hardroxide doctor`: check the Vitis tools, JTAG access and the console port.
- Runtime crates (`mmio`, `fpga`, `dma`, `irq`, `ip`) are `no_std`-capable (`std` feature, on by
  default); `IrqLine` gained `edge`; `mmio::time` is a registered time source.

### Verified
On a PYNQ-Z2 and under QEMU; see `docs/08-verification-gaps.md`. A two-hour run of the kernel under load (a
processor hog and a cache-thrashing task): the 10 ms task was released at most 10.2 us late with no missed release, and an
interrupt reached its waiting task within 5.4 us. `hardroxide package` has built a
BOOT.BIN but it has not yet been booted from SD.

### Documentation
- Tutorials are grouped by flavor: eleven for Linux (one per test design), three for bare metal and one for
  the RTOS, with output captured on a PYNQ-Z2. New reference pages for `hardroxide-ps7`, `hardroxide-rt` and the
  bare-metal crate; a "Choosing a flavor" section in the workflow guide; a page on the end-to-end workflow and one
  on exporting an XSA from Vivado.
- The documentation site has a landing page, with the book under `/book/` and rustdoc under `/api/` (old book
  URLs redirect).
- Stale statements corrected (hardware status, cross-building).
- Known issue G17 (PS-PL HP port can wedge after many design switches) documented.

### Tooling
- `scripts/board/{prep,overlay,run}.sh`: free the PL, apply the interrupt overlay,
  cross-build and run on the board.
- `fixtures/variants/` (update tutorial) and a test that compiles every program in
  `fixtures/board_tests/` against a freshly generated crate.
- The DMA `StuckChannel` error now says what to do about a wedged HP port.

## 1.0.0 (2026-11-16)

First release.

### Generator and CLI
- `hardroxide generate`, `update`, `inspect` for Zynq-7000 XSAs (Vivado 2025.2
  exercised; HWH from 2019.2 to 2025.2 parsed). Other architectures are refused.
- Deterministic output; interface hash (including the parameters generated driver
  constructors use) and bitstream hash, so `update` can swap a bitstream alone.
- Only the Processing System's address map is used: MicroBlaze-private
  peripherals no longer get PS addresses.
- Examples take an instance name when a design has several of a core.
- `inspect` reports what each instance becomes, with interrupt routing.

### Runtime
- Drivers: GPIO, HLS (`ap_ctrl_hs`/`ap_ctrl_chain`, pointer arguments), DMA
  (direct register mode), Timer, IIC, Quad SPI, UART Lite, BRAM controller,
  interrupt controller, AXI4-Stream FIFO.
- MMIO backends: `/dev/mem`, UIO, mock. DMA buffers over `u-dma-buf`.
- Interrupts over UIO (blocking and async), including fan-in through an AXI
  interrupt controller, with a generated device-tree overlay.
- `load()` refuses to reprogram the PL while a kernel driver is bound to a PL
  address.

### Fixed during hardware validation
- IIC: the status register is toggle-on-write; clearing it by writing `0xFF`
  set every bit and made every transfer report a NACK.
- DMA: a channel waiting on an empty stream ignored the stop request and stayed
  busy; the driver now soft-resets the core.
- FIFO: the received length is rounded up to whole words (documented).
- A bitstream-only update could leave stale DMA/SPI/Timer constructor values
  compiled in.

### Known limits
- Zynq-7000 only; Linux userspace; DMA scatter-gather, HLS stream and array
  ports, and IP with several register blocks are not supported.
- A GIC interrupt line keeps the trigger type of the first overlay that mapped it
  until reboot; on the validated image SPI 29 (`IRQ_F2P[0]`) is already mapped
  level-triggered at boot.
