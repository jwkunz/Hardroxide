# Changelog

## Unreleased

Applications on the PYNQ base overlay, versions 2.2 to 2.5 of [ADR 0004](docs/decisions/0004-base-overlay-applications.md); the workspace
version is 2.5.0, not yet released (the release waits for the rest of the examples). Verified on a PYNQ-Z2; the open items are listed.

### HDMI output and graphics (2.2)
- `hardroxide-ip`: `AxiVdma` (read channel, park-mode double buffering), `VideoTimingController` (generator) and `AxiDynclk` (the
  reconfigurable pixel clock); the generator uses them for the base overlay's video cores.
- `hardroxide-video`: display modes (640x480 to 1920x1080 at 60 Hz) and `Display`, the bring-up of the HDMI output.
- `hardroxide-gfx`: a small 2D graphics library over a pixel buffer (shapes, blits, a 5x7 font).
- `hardroxide-ps7`: `mem::set_non_cacheable`, for a buffer a PL master uses at a fixed address.
- `apps/base`: `video_demo` (bare metal, 60 fps double buffering), `video_modes`, and `rt_video` (RTOS, paced by the frame interrupt).
- Verified: 800x600, 1024x768, 1280x720, 1280x1024 and 1920x1080 display; 60 fps; `rt_video` frame interval within 10 us of 16.667 ms under load.
  **Open item: 640x480 is rejected by the tested monitor.** The first run found, by reading the Linux desktop's registers over JTAG: the clock
  generator core in the overlay has no reference-frequency register; the video DMA reaches DDR only at `0x10000000` to `0x1FFFFFFF`; the VDMA
  start sequence and the timing controller's polarity and vsync values.

### Audio (2.3)
- `hardroxide-ps7`: the PS I2C controllers (`i2c`, also an `embedded-hal` I2C bus) and `clocks::cpu_1x_hz`.
- `hardroxide-ip`: `AudioCodecCtrl`, the PYNQ audio codec controller (I2S, one frame each way, no FIFO).
- `hardroxide-audio`: the ADAU1761 codec over I2C, WAV parsing and writing, tones, resampling, playback, recording, pass-through and full-duplex `play_record`.
- `apps/base`: `audio_tone`, `audio_wav`, `audio_record`, `audio_loopback`, `audio_loopback_test`, `audio_record_test` and a generated `chime.wav`.
- Verified: tones, WAV playback, a line-in loop with a cable from HPH OUT to LINE IN, recording to memory and playing it back, no missed samples.
  Not run: `audio_record` and `audio_loopback` (they need a separate source and headphones), the microphone input, RTOS audio (not written).
  Fixed on the board: unbounded recursion in the I2C `embedded-hal` impl; the missed-sample counter.

### SD card and FAT32 (2.4)
- `hardroxide-mmio`: the `BlockDevice` trait (512-byte blocks) and `SliceDisk`, a disk in memory.
- `hardroxide-ps7`: `sd::SdHost`, the PS SD controller (SDHCI) in programmed I/O: card identification, 4-bit bus at 25 MHz, block reads and writes; `clocks::sdio_ref_hz`.
- `hardroxide-fat` (new): FAT32 over any block device: mount (MBR or bare volume), long names, directories, read, write, append, seek, truncate, remove, format. Tested against
  `mkfs.fat` and `fsck.fat -n` (CI now installs `dosfstools`) and under QEMU's SD card (`baremetal/sd-smoke`).
- `hardroxide-gfx`: BMP reading and writing (`gfx::bmp`); it now depends on `thiserror`.
- `apps/base`: `sd_info` (read only), `sd_erase_and_test` (formats a card; refuses a card that holds data unless its serial number is named at build time), `sd_logger`, `sd_wav`, `sd_bmp`.
- Verified on a PYNQ-Z2 with a 32 GB SDHC card: identification, raw transfers, format (15 s), nine files byte for byte, the three applications; 7.4 MB/s read and 1.6 MB/s write.
  Open: the board-formatted card has not been through `fsck.fat` on a PC; SDSC cards, the second controller and the RTOS flavor are not covered.
- Fixed on the board: the PS configuration put MIO bank 1 (SD card, Ethernet, USB) at 3.3 V instead of the PYNQ-Z2's 1.8 V, so every SD command timed out (the board preset now carries
  `PCW_PRESET_BANK0_VOLTAGE`/`PCW_PRESET_BANK1_VOLTAGE` and all board build scripts were regenerated); the CID's product name and serial were decoded one byte early.

### Ethernet and a web server (2.5)
- `hardroxide-ps7`: `gem`, the Gigabit Ethernet MAC (Cadence GEM): descriptor rings, polled frames, MDIO and PHY autonegotiation, link speed and clock; its `net` feature implements the network stack's `Device`.
- `hardroxide-net` (new): our own IPv4 stack: Ethernet, ARP, ICMP echo, UDP, a DHCP client with a static fallback, TCP (retransmission, windows, several connections) and an HTTP/1.1 server. `no_std`, no allocation, no dependencies.
  Tested against itself over a lossy virtual wire, against 30,000 random frames, and under QEMU against real `curl` and Python sockets.
- `apps/base`: `net_ping`, `net_udp`, `net_web` (bare metal) and `rt_net` (a web server beside a 10 ms control loop on the RTOS).
- Verified on a PYNQ-Z2 cabled to a PC: link at 1000 Mbit/s, DHCP, ping at 0.28 ms, about 3,200 web requests/s, and 21,879 requests in 35 s with the control loop's worst deviation staying at 1 us.
  Open: a browser, 10 and 100 Mbit/s links, a router in between, hours of traffic.
- Fixed on the board: the PS preset left the Ethernet pins, the MDIO pins and the MAC clock unconfigured (Vivado reset them to EMIO when the preset was applied one key at a time); the preset is now applied in two passes
  and the base overlay's `ps7_init` was regenerated (the other board fixtures were not).
- Fixed by testing against `curl`: a busy server ignores extra SYNs instead of resetting them; a finished connection no longer holds a server socket in `TIME-WAIT`.

### Tooling
- `scripts/test-board.sh` also runs the video and audio programs (audio loopback with `HRX_AUDIO_LOOPBACK=1`).

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
