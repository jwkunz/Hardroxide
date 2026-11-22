# 08 - Verification gaps (no hardware available)

Everything below was implemented without a PYNQ-Z2 attached. Each item says
what was assumed, what evidence exists, how to verify on the board, and what
to change if the assumption is wrong. Work through the list in order: later
items depend on earlier ones. When an item is verified, change its status here
and record the date and image/kernel version.

Status key: **VERIFIED-OFFLINE** (checked against a vendor tool, no board
needed), **UNVERIFIED** (needs the board), **DESIGN-GAP** (known incomplete).

Prerequisites on the board: stock PYNQ-Z2 image, root shell, and either a Rust
toolchain on the board or a cross-compile
([Getting started](guide/getting-started.md#build-for-the-board-linux-flavor)). Record
`uname -a`, `ldd --version` and the PYNQ image version here.

## Verified offline

| ID | Item | Evidence |
|---|---|---|
| V1 | `.bit` -> `.bin` conversion (header strip, per-word byte swap, NOP padding to 16 bytes) | Output of `hardroxide generate` is byte-identical to `bootgen -arch zynq -process_bitstream bin` (Vivado 2025.2) for `fixtures/pynq_z2_gpio.xsa`. Reproduce: bif `all:{pynq_z2_gpio.bit}`, `bootgen -image a.bif -arch zynq -process_bitstream bin -w on`, `cmp` against `bitstream/design.bin`. Whether the *kernel* accepts this format is G1. |
| V2 | HWH parsing (instances, registers, fields, address map, FCLK, interrupt nets) | Eight 2025.2 fixtures (GPIO, HLS scalar, HLS `m_axi`, DMA, Timer/IIC/SPI, interrupt controller, BRAM/UART, AXI4-Stream FIFO); every driver's register offsets and control bits are asserted against them. Real third-party designs and older Vivado versions are covered in D5. |
| V3 | Generated crate compiles warning-free; mock-MMIO behaviour; singleton semantics; load-failure releases the singleton | `cargo test -p hardroxide-gen` generates, builds and tests a real crate. |
| V4 | `/dev/mem`-style mmap path (page alignment, offsets, volatile access) | Unit test maps a regular file at an unaligned offset. Does not exercise real `/dev/mem`. |
| V5 | Runtime crates work as 32-bit ARM binaries | `scripts/test-armv7.sh` builds the runtime crates' tests for `armv7-unknown-linux-gnueabihf` and runs them under `qemu-arm` user-mode emulation (in CI too). Includes mapping a window above 2 GiB (the SLCR at `0xF800_0000`) through a 32-bit `off_t`. Emulates the CPU and libc only, not the Zynq or its kernel. |

## Board results

First hardware session: **2026-11-16**, PYNQ-Z2, image "PYNQ 3.1 (Carlisle)
2025_07_10", kernel `6.6.10-xilinx-v2024.1-gb36799f4e960` (armv7l), glibc 2.35
(Ubuntu), cross-built binaries copied over SSH. The image is a custom "panther"
build whose base overlay drives HDMI, audio and LEDs from the PL (see G16).

| Gap | Result |
|---|---|
| G1 bitstream load | **Passes** once the base design's drivers are stopped (G16); freezes the board otherwise |
| G2 FCLK readback | **Passes**: FCLK0 reads 100 000 000 Hz; on a second design FCLK0 = 50 MHz and FCLK1 = 142.857 MHz both read back correctly from the SLCR, and a Timer built for 50 MHz measures 100.018 ms |
| G4 `/dev/mem` and `Uio` mapping | **Passes**: PL registers read and write through `/dev/mem`; the `Uio` backend maps the Timer through `/dev/uioN`, runs a 100.019 ms delay, and sees the same counter as the `/dev/mem` view |
| G5 GPIO | **Passes**: LED0 to LED3 light in order (seen by eye); buttons 0 to 3 read as bits 0 to 3 and both switches as bits 4 and 5 (`0b[sw1 sw0 btn3 btn2 btn1 btn0]`), all switch combinations seen |
| G9 HLS scalar | **Passes**: `vadd` computes 1+2, 100+23, 0xFFFFFFFF+1 (wraps), 7+35; done interrupt fires and re-arms |
| G11 interrupts | **Passes**: the generated `interrupts.dtso` applies unchanged through configfs; `irq_probe`/`board_check` open all UIO devices; Timer and HLS interrupts fire and re-arm |
| G12 UART interrupts | **Pass**: RX interrupt delivers a byte about 130 us after it is sent (three rounds, re-arming), TX-empty interrupt fires, no error flags. IIC NACK interrupt (`ISR` int1) fires and re-arms over three rounds |
| G12 Timer, SPI, IIC | Timer **passes** (100 ms asked, 100.017 ms measured; interrupt 50 ms +0.08 ms). SPI internal loopback **passes** (40 bytes). IIC **passes** on the loopback fixture (two cores on one in-fabric bus): a 5-byte write reaches the target at `0x2A` intact, an unanswered address NACKs, a bus scan finds only `0x2A`. This found a driver bug, fixed: the IIC `ISR` is toggle-on-write, and `init` wrote `0xFF`, which *set* every status bit, so every transfer reported a NACK |
| G13 AXI intc | **Passes**: Timer interrupt through `axi_intc`, `pending`/`ack`/timeout |
| G14 BRAM / UART | BRAM **passes** (8 KiB pattern, 0 mismatches). UART **passes** on the loopback fixture (two cores cross-wired in the fabric): 256 bytes each way, no overrun/frame/parity errors, 22 ms per 256 bytes (115200 baud). Interrupt-driven UART not tested |
| G15 FIFO | **Passes**: lengths 1 to 65 bytes and the maximum 2032-byte packet round-trip (the length is reported rounded up to whole words, data intact, padding zero); an oversize packet is refused with nothing written; `ISR` flags clear when written with 1s and are untouched by 0 (receive and transmit complete read `0x0C000000`); interrupt-driven receive fires and re-arms over three rounds |
| G10 DMA | **Passes**: loopback matches for lengths from 4 bytes to 1 MiB, 200 back-to-back 4 KiB transfers with 0 mismatches, 400 MB/s on a 1 MiB transfer (the limit of a 32-bit stream at 100 MHz); interrupt-driven completion works on both channels and re-arms. Found and fixed: after a timeout on an S2MM channel with no sender, the channel ignored the stop request (`StuckChannel`, then `Busy`); `halt` now soft-resets the core and the next transfer works |
| G9 HLS pointer arguments | **Passes**: `scale` reads one `u-dma-buf` buffer and writes another through `set_*_buffer` (256 elements, 0 mismatches) |
| Real third-party designs | The PYNQ `base` overlay (about 70 IP instances, four MicroBlaze subsystems, video, DMA) and `logictools` generate crates that cross-build for armv7 without warnings. Found and fixed: the parser used every master's address map, giving MicroBlaze-private peripherals bogus PS addresses that collided (four different instances at `0x41C00000`), which could hang the CPU; only the PS's own map is used now. The base crate then ran on the board: all four FCLKs read back (100, 142.857, 200, 100 MHz), switches, buttons and LEDs work through typed GPIO drivers, the system interrupt controller reads, and the generated BRAM test passes on a 64 KiB MicroBlaze memory |
| G17 HP port wedge | Observed twice after many design switches; cleared only by power cycle; cause not found (see G17) |
| `hardroxide-ps7` (bare metal, 2026-11-20) | **Passes** over JTAG on the PYNQ-Z2 after the fixture's `ps7_init`, and under QEMU: startup on core 0, per-mode stacks, vector table, console at 115207 baud, clocks (CPU 650 MHz, private peripherals 325 MHz), `sleep(100 ms)` = 100.07 ms, 20 private-timer interrupts in 200.04 ms through the GIC with none unhandled, and an undefined-instruction report with registers. MMU and caches are still off |
| Generated bare-metal crate (2026-11-20) | **Passes** on the PYNQ-Z2 over JTAG (`ps7_init`, bitstream, `ps7_post_config`, load): `Design::init()` finds the PL configured, `board_check` prints the interface hash and FCLK0 = 100 MHz, `gpio_smoke` walks the LEDs and reads the inputs, all through the generated crate. Needed a board-accurate PS7 setup for the console (MIO 14/15, UART clocks). Finding: applying the PYNQ base overlay's *DDR* settings through Vivado produced a DDR that failed a 16 MiB memory test (it never finished in 60 s), while the fixtures' stock-part DDR settings pass it in 8 s; the board fixture scripts therefore copy everything except DDR |
| Bare-metal PL interrupts and `hardroxide run` (2026-11-20) | **Pass**: `hardroxide run` loads the generated `board_check`, `timer_delay` and `timer_irq` from the peripherals board fixture with one command. `board_check` lists the interrupt routing (GIC IDs 61/62/63, level/level/edge from the design); the AXI Timer's interrupt reaches a bare-metal handler through the PL-to-PS path, three events at about 50.04 ms each with cumulative counts; a 100 ms hardware delay measured 100.033 ms |
| `hardroxide-rt` kernel (2026-11-20) | **Passes** on the PYNQ-Z2 over JTAG and under QEMU (`baremetal/rt-smoke`): a priority-5 task is released through a lock held by a priority-1 task while a priority-3 task hogs the processor and gets it 30 ticks later (inheritance; without it about 100); a 10 ms periodic task shows 0 ticks of lateness over 20 releases; 20 wakeups of a 5 ms sleeper preempt a task that never yields with none late; a queue passes 10 items in order; a semaphore given from an interrupt handler wakes its waiter exactly 20 ticks after the trigger. Tick 1 kHz. Jitter and long-run stability are not measured yet |
| Bare-metal tutorials 2 and 3 (2026-11-21) | **Pass** on the PYNQ-Z2 over JTAG: `timer_ticks` (200 interrupts from an auto-reloading PL timer at 10 ms: gaps 9999 to 10001 us), `timer_delay` (100 ms asked, 100.003 ms measured), `timer_irq` (50.002 to 50.004 ms), `dma_loopback` (4096 bytes), `dma_speed` (100 x 16 KiB, 0 mismatches, DMA 345 MB/s; the processor filling and comparing non-cacheable buffers limits the whole loop to 4 MB/s) |
| Base overlay on bare metal and RTOS (2.1, 2026-11-21) | **Passes** on the PYNQ-Z2 over JTAG: the board-ready base overlay (with a `ps7_init` built from the PYNQ-Z2 preset) boots, `board_check` reports all four clocks (FCLK1 142.857142 MHz for the design's 142.857132) and the controller interrupt as GIC 61 (level). `selftest`: LEDs and RGB LEDs, and a software-raised interrupt-controller input reaches the processor through the GIC in 4 us (polled). `rt_irq_latency`: 2000 simulated interrupts to a priority-6 task through the kernel's `IrqEvent`, with a hog and a cache-thrashing task running: mean 4.2 us, worst 6.2 us. `rt_lights`: the 1 ms PWM task's worst lateness 1 us. **Buttons and LEDs verified by hand (2026-11-21):** `lights`: all four buttons raise the GPIO interrupt, which reaches the program through the interrupt controller and the GIC (button 0 counts up on the four LEDs, 1 counts down, 2 clears, 3 lights the RGB LEDs in the colour the switches choose; the switch value is read at each press); the maintainer saw the LEDs and RGB LEDs behave as described. `rt_lights`: 16 presses in a minute changed the pattern and the brightness, the kernel's PWM task's worst lateness was 7 us with button traffic (1 us without), and the maintainer confirmed the lights behaved as described. **Not tested**: the RGB channel order was not examined separately. The base overlay's own DDR settings, applied through Vivado, fail a memory test; the preset's are used |
| HDMI output and graphics (2.2, 2026-11-21) | **Passes on the PYNQ-Z2 and a monitor, after fixes found on the board.** `video_demo` at 1280x720: pixel clock 74.2856 MHz (0.05 % high), a stable test pattern with the right colours, then a double-buffered animation at **60 frames per second**, no DMA errors. `rt_video` under the kernel, paced by the VDMA's frame interrupt through the interrupt controller: 60 fps, drawing about 3 ms, frame interval worst deviation **10 us** from 16.667 ms with a load task running. The modes 800x600, 1024x768, 1280x720, 1280x1024 and 1920x1080 display; **640x480 does not** (the monitor shows its test pattern at both 25.175 and 25.2 MHz; open item). What the first run (blank screen, cable not detected) found by comparing against the PYNQ desktop's registers read over JTAG at 1080p: the `axi_dynclk` core is the older eight-register version (the reference-frequency register does not exist; the reference is FCLK0); the VDMA reaches DDR only at `0x1000_0000` to `0x1FFF_FFFF` (a decode error otherwise); the VDMA start sequence and the VTC polarity and vsync values needed to match Linux's. Not tested: cached frame buffers with explicit cache maintenance, the HDMI input path, the HP-port wedge (G17) under sustained VDMA traffic (an hour-long run), other monitors |
| Audio (2.3, 2026-11-21) | **Passes on the PYNQ-Z2 with headphones and a cable from HPH OUT to LINE IN**, after two fixes found on the board. The codec's PLL locked on the first attempt (so the PS I2C 1 controller reaches the ADAU1761 through the EMIO: 400 kHz requested, 378,787 Hz programmed). `audio_tone`: a C major scale and a chord heard through headphones, 0 missed samples in every note. `audio_wav`: an 8 kHz chime resampled to 48.8 kHz and played twice, clean and at the right pitch, 0 missed. `audio_loopback_test`: a 1 kHz tone out of HPH OUT and back through LINE IN, 1000 Hz counted on both channels at about 11 % peak, 0 missed in 146,484 frames. `audio_record_test`: the tone recorded into memory, played back from memory and recorded again, 1000 Hz and 999 Hz, 0 missed. Found on the board: unbounded recursion in the PS I2C `embedded-hal` impl (`transaction` resolved `self.write` to the trait's default, a stack overflow the host tests could not see), and an inverted ready-flag test in the missed-sample counter. The pink MIC jack did not hear a cabled tone (it is a differential microphone input). **Not run**: `audio_record` and `audio_loopback` (they need a separate source and headphones), the microphone input, other sample rates, RTOS audio (not written), the absolute sample rate against a reference (frequencies are counted against the program's own clock) |
| SD card and FAT32 (2.4, 2026-11-21) | **Passes on the PYNQ-Z2 with a SanDisk 16 GB and a 32 GB SDHC card**, after two fixes found on the board. Host and QEMU: 20-odd tests of `hardroxide-fat` against `mkfs.fat`/`fsck.fat -n` (many-entry directories, long names, an 8 GB sparse layout, damage detection) and the SD driver under QEMU's SD model, whose image `fsck.fat` accepts. Board: the controller identifies the card and runs at 25 MHz on four bits; `sd_info` mounted a card formatted by another device and listed its directory and a 1 GB file; `sd_erase_and_test` on the 32 GB card: raw transfers of 1, 4 and 64 blocks, format in 15.3 s (16 KiB clusters), nine files of 0 to 100000 bytes byte for byte, 4 MiB at 7.4 MB/s read and 1.6 MB/s write; `sd_logger` over two boots (10, then 20 lines), `sd_wav` (written to the card, loaded, played twice, 0 missed) and `sd_bmp` (a 1280x720 screen saved as a 2.76 MB BMP, reloaded, 0 pixels differ). Found on the board: **MIO bank 1 must be 1.8 V** on the PYNQ-Z2 (SD, Ethernet and USB); the preset had 3.3 V, so every command timed out with a command conflict (found by reading `MIO_PIN_40` to `45` from the Linux image over JTAG: `0x1280` against our `0x1680`), and the CID decoding was one byte early (found by comparing with Linux's `/sys/block/mmcblk0/device`). Also: a Netac 8 GB microSD HC did not answer on the board or in a PC reader (the card or its adapter). **Incident:** the first version of the format test was run on a card in the slot that held a PYNQ image, because "no FAT32" was taken to mean "blank"; the card had to be re-flashed. The test now refuses any card with data unless its serial number is named at build time. **Not run**: `fsck.fat` on a card the board formatted, SDSC cards, the 1-bit bus, the second controller, UHS cards, RTOS use, removal during a transfer, opening `screen.bmp` in a PC viewer |
| Ethernet and web server (2.5, 2026-11-21) | **Passes on the PYNQ-Z2 cabled straight to a PC**, after one configuration fix. Host: `hardroxide-net` tests (200 kB over a clean virtual wire; 30 kB over wires losing every 11th, 5th and 3rd frame; ping, ARP, resets, busy-port behaviour, DHCP and its fallback, the web server, 400 and 413; 30,000 random and corrupted frames with overflow checks on) and the MAC driver's ring logic with a fake DMA region. QEMU: the stack on the emulated MAC against real `curl` and Python (DHCP, 300 kB byte for byte, POST, 40 requests, 6 concurrent, UDP and TCP echo). Board: PHY (Realtek RTL8211E, ID `0x001cc915`) found at MDIO address 0, link 1000 Mbit/s full duplex, DHCP from the PC (192.168.2.135), `ping` 5 of 5 at 0.24 to 0.34 ms, `net_web`: `/api/status` JSON, `/led?v=5` set the LEDs, a 115 KB BMP picture, 3,193 requests/s sequentially and 5,051/s with five clients (400 of 400 succeeded), 7.2 MB/s for the picture; `rt_net` (RTOS): 21,879 requests in 35 s from six clients with 0 errors while the 10 ms control task's worst deviation stayed at 1 us over about 5,900 releases (also 1 us with no load). Found on the board: the PS preset applied Ethernet 0 as EMIO with an external clock (MIO 16 to 27 and 52, 53 tri-stated; every MDIO read returned 0), because Vivado reset the pin group when the preset keys were applied one at a time; fixed by applying the preset twice, `fixtures/board/pynq_base_overlay.xsa` rebuilt, the other board fixtures not. Found by the QEMU clients: a busy server must ignore extra SYNs (a reset made `curl` give up), and `TIME-WAIT` must not hold a server socket. Also found: `scripts/test-board.sh` had skipped the video and audio blocks since 2.2 (its build of all applications failed without the extra crates). **Not run**: a browser, 10 and 100 Mbit/s and half duplex (written, not exercised), a router or switch in the path (DHCP was the PC's), unplugging the cable while running, a soak of hours, the second MAC, jumbo frames, IPv6 |
| HTTPS (2.6, 2026-11-21) | **Passes on the PYNQ-Z2 with `curl`, `openssl s_client` and Microsoft Edge (headless)**, after one fix found by the browser. Host: `hardroxide-crypto` against OpenSSL and Python `cryptography` on more than 800 random vectors and the RFC examples (hashes at every length around the block boundaries; HMAC; HKDF; ChaCha20-Poly1305 including rejection of altered tags, ciphertext and aad; X25519; Ed25519; P-256 public keys and **70 RFC 6979 ECDSA signatures byte for byte equal to OpenSSL's**), `hardroxide-tls` against `openssl s_client` and `curl` over TCP (Ed25519 and P-256 certificates; refusals; 200 random streams), `hardroxide make-cert` (OpenSSL verifies the signature; `curl` fully verifies by DNS name and by IP and refuses a wrong name). QEMU: the whole stack against `curl` and `openssl s_client` (a page, 100,000 bytes byte for byte, POST, 30 handshakes, 6 concurrent over 4 sockets, refusal of a client without ChaCha20, garbage and an idle connection). Board: `net_https` over TLS 1.3 (`TLS_CHACHA20_POLY1305_SHA256`, `ecdsa_secp256r1_sha256`, X25519) to curl, s_client and Edge; X25519 26 ms, P-256 signature 22 ms, Ed25519 signature 44 ms; 10.9 requests/s sequential (83 ms, each a full handshake) and 12.5/s with 4 clients (100 of 100 succeeded); `rt_https`: 489 TLS requests in 40 s from 3 clients, 0 errors, control loop worst deviation 2 us over ~6,900 releases. Found by Edge: **Chromium offers no Ed25519 signature algorithm** (captured ClientHello: ECDSA P-256/P-384, RSA-PSS, RSA; X25519 with a post-quantum share), so the Ed25519-only server was refused (`ERR_SSL_VERSION_OR_CIPHER_MISMATCH`); fixed by adding ECDSA P-256, which matched OpenSSL on the first run. **Not run / known weak**: Firefox and Safari, an interactive browser session, certificate chains, resumption, hours of traffic; the random bytes are **weak** (clock, connection timing and a card-held seed; the Zynq has no hardware generator); "constant time" is by construction and not measured; no independent review; handshakes cost ~80 ms of the single core so a few connections a second keep the server busy |
| Integration (2.7, 2026-11-21) | **Passes on the PYNQ-Z2**, after three bugs that only showed with everything running. `rt_station` (RTOS: an HTTPS dashboard, HDMI 720p at ~60 frames/s, audio by timer interrupt at 97.656 kHz, a 10 ms control task, a processor burner): with three HTTPS clients (392 status requests and 21 screen grabs in 45 s, 0 errors), the burner on, six display-mode changes, audio stop and start and LED writes, the control loop's worst deviation was **4 us over 5,179 releases** (3 us before), the audio pump wrote 2,528,966 frames with **0 underruns**. The audio pump alone (`rt_audio`, with a processor hog): 1.47 million frames in 30 s, 0 underruns, interrupts **10.4 %** of the processor, the feeder 1.1 %, a constant -17 ppm between the codec's clock and the processor's, the control loop 3 us. **Soak: 17 minutes (planned: two hours; stopped for the release)**: 1,202 requests, 0 failed, 55 bursts of six concurrent handshakes, 105,866 control releases with the worst deviation 4 us in every minute, 51.7 million audio frames with 0 underruns, 66,164 display frames, every task's stack high-water mark identical in the first and last minute. Found: the first per-task processor accounting (sampling at the kernel tick) was wrong (the 10 ms task showed 0 %, the burner 99 %, interrupt time was charged to the interrupted task), replaced by time stamps at every context switch with interrupt time counted separately; `task_info` scanned a 256 KiB stack with interrupts masked and delayed the control loop by 409 us on every status request (found as a one-off spike by recording what the other subsystems were doing, fixed by scanning outside the critical section); the network task used 87 % of the processor for lack of a sleep (the new monitor showed it at once); the on-screen panel was repainted in one of two frame buffers (it flickered between versions; seen in a screen grab fetched through the web server). **Not run / not done**: the **audio was never listened to** (counters only), an interactive browser session, **HDMI input capture** (needs a VDMA write-channel driver and the overlay's HDMI receiver bring-up, which do not exist), a soak of hours, the display's frame times during TLS handshakes alone |
| RTOS jitter, 2 hours (2026-11-21) | **Passes** (`baremetal/rt-jitter`, board over JTAG, 121 minutes, 7.26 million kernel ticks). Load: a priority-1 task that never sleeps, a priority-3 task thrashing a 4 MiB array (past the 512 KiB L2) for about 3 ms of every 10 ms, and a priority-2 reporter. **Release lateness of the 10 ms task** (726,197 releases, measured against the 3 ns global timer): mean 1.06 us, maximum 10.2 us; 91.7 % under 1 us, 8.3 % between 8 and 12 us, none above 12 us; **0 missed releases**. **Interrupt-to-task latency** (907,875 software interrupts raised at 7 to 9 ms intervals, through the IRQ entry, a semaphore give and the context switch to a priority-7 task): mean 3.95 us, maximum 5.44 us, 99.99 % under 5 us. Neither maximum moved after the first minute, so the first minute already held the worst case seen. Not covered: PL-originated interrupts under load, a second master hammering DDR, temperature extremes |
| Generated RTOS crate (2026-11-21) | **Passes** on the PYNQ-Z2: `hardroxide generate --platform rtos` from the board-preset GPIO design, built and run with `hardroxide run`; the 10 ms control task's period deviated 1 us at most over 700 releases (measured against the global timer, so it shows release regularity, not interrupt latency). Every fixture's RTOS crate builds for the target in the test suite |
| MMU, caches, L2 (2026-11-21) | **Passes** on the PYNQ-Z2 and under QEMU: 20 passes over 64 KiB take 100.7 ms with the caches off and 4.4 ms on; the bare-metal `timer_irq` and the RTOS example still pass; a generated `dma_loopback` moves 4096 bytes through `.dma` buffers with the caches on. Not tested: cached buffers with explicit `clean_range`/`invalidate_range` against a PL master, SCU/ACP, multi-core |
| G16 guard | **Passes**: with `gpio-xilinx` rebound to `41200000.gpio`, `load()` refuses with `PlInUse` naming the device and does not touch the PL; after unbinding it loads. Passive `uio_pdrv_genirq` devices at PL addresses are correctly ignored |
| Overlay trigger types | A GIC line keeps the trigger type (level/edge) of the first overlay that mapped it, even after the overlay is removed; a later overlay with the other type fails with `irq: type mismatch, failed to map hwirq-N` and the UIO device then returns EIO. Switching between designs with different types on the same line needs a reboot |
| `u-dma-buf` | The image does not ship it; built for this kernel, see [Running on hardware](guide/hardware.md#building-u-dma-buf) |
| G8 cross-build | **Passes**: Vitis `arm-linux-gnueabihf-gcc` as the linker; binaries need glibc 2.34 and run on the board's 2.35 |
| `hardroxide update` | **Passes**: replacing only the bitstream (same interface, different `USERID`) leaves `src/` and `Cargo.toml` byte-identical, changes `design.bin`'s hash, and the crate loads and runs; a design with a different FCLK0 is refused with a diff because the Timer's constructor clock is part of the interface hash. An implementation-only change (placement directives) can produce an identical bitstream, which `update` reports as "up to date" |
| Async interrupts | **Pass**: `UioIrq::wait_async` under a minimal executor wakes three times at about 40 ms and returns `None` on timeout |
| Register access cost | user-space `/dev/mem` read 193 ns, write 231 ns per access (uncached, 200 000-access average) |
| G3 PL reset | **Passes**: a Timer left running is back at its reset state (`TCSR0 = 0`, counter 0) after the PL is reprogrammed |
| G7 singleton | **Passes**: a second `load()` while one `Design` is alive returns `AlreadyLoaded` and leaves the first working; 25 load/delay/drop cycles in one process, loads about 300 ms each |
| Base-design line mapping | On this image SPI 29 (`IRQ_F2P[0]`) is already mapped level-triggered at boot, so an overlay node that needs it as edge (for example an AXI UART Lite on F2P bit 0) gets a UIO device whose reads and writes return `EIO`, with no kernel message. Put edge-triggered cores on other F2P bits |
| G6 | Not exercised |

## Needs the board

Start with the generated `board_check` example: it runs G1 (load), a G2 FCLK
readback against the design, and the G11 UIO-device check in one go, and
touches no IP register.

### G1. fpga_manager accepts the `.bin` and programs the PL (VERIFIED 2026-11-16, see Board results)
- Assumed: write `0` to `/sys/class/fpga_manager/fpga0/flags`, copy bin to
  `/lib/firmware/<name>`, write `<name>` to `.../fpga0/firmware`, then
  `.../state` reads `operating` (`hardroxide-fpga::load_bitstream`).
- Check: `cat /sys/class/fpga_manager/fpga0/{name,state,flags}`; manually
  `cp bitstream/design.bin /lib/firmware/t.bin; echo t.bin > .../firmware`;
  `dmesg | tail`. Then run `cargo run --example gpio_smoke`.
- If wrong: the device name may differ (`fpga0`), `state` may report
  differently (e.g. `write complete`), the `flags` attribute may be absent on
  this kernel (then skip it), or the image may need the *unswapped* bin / the
  legacy `/dev/xdevcfg` path (PYNQ's older images). Fix in `hardroxide-fpga`;
  if unswapped is needed, change `BitFile::to_bin_swapped` and V1.
- Also: the firmware file is written to `/lib/firmware` and deleted after
  load; confirm the firmware path search works when `firmware_dir` is the
  default, and whether the kernel needs `firmware_class.path`.

### G2. PL clock programming via SLCR (VERIFIED 2026-11-16, see Board results)
- Assumed (UG585): SLCR at `0xF8000000`; unlock `0x008`<-`0xDF0D`, lock
  `0x004`<-`0x767B`; `ARM/DDR/IO_PLL_CTRL` at `0x100/0x104/0x108` with
  `PLL_FDIV` in bits 18:12 and `PLL_BYPASS_FORCE` bit 4; `FPGAn_CLK_CTRL` at
  `0x170/0x180/0x190/0x1A0` with `SRCSEL` bits 5:4 and `DIVISOR0/1` in bits
  13:8 / 25:20; PS_CLK from the HWH (`hardroxide-fpga::slcr`).
- Check: before running, read `devmem 0xF8000170`, `0xF8000108`; run the
  program; confirm FCLK0 = 100 MHz (e.g. `cat /sys/kernel/debug/clk/...` if
  available, or measure an AXI-clocked counter, or compare with PYNQ's
  `Clocks.fclk0_mhz` if Python is available).
- Risks: SRCSEL encoding (0/1 = IO PLL, 2 = ARM PLL, 3 = DDR PLL). The PS
  reference clock is taken from the HWH (`PCW_CRYSTAL_PERIPHERAL_FREQMHZ`,
  emitted as `PS_CLK_HZ`); PYNQ's own base design uses 50 MHz, and the fixture
  was rebuilt with 50 so it matches the board. Confirm the real board crystal.
  Writing SLCR through `/dev/mem` may conflict with a kernel clock driver that
  caches the rate; check `dmesg` and that other peripherals are unaffected.
- Also check whether `fpga_manager` load resets/gates the FCLKs, which would
  require applying clocks *after* load (current order) or re-enabling them.

### G3. PL reset (FCLK_RESET) handling (VERIFIED 2026-11-16 for the reload case, see Board results)
- Not implemented. The HWH exposes `PCW_EN_RSTn_PORT` (modeled as
  `PlClock::reset_enabled`) but `load()` does not touch `FPGA_RST_CTRL`
  (SLCR `0x240`). Check whether the fabric is in reset after load; if so,
  deassert via SLCR (unlock, clear bits, lock) and add to `hardroxide-fpga`.
  Also AXI level shifters (`LVL_SHFTR_EN`, SLCR `0x900`) may need enabling.

### G4. `/dev/mem` and UIO mapping of PL addresses (VERIFIED 2026-11-16, see Board results)
- Assumed: `open("/dev/mem", O_RDWR|O_SYNC)` + `mmap` at `0x41200000` works
  as root (no `CONFIG_STRICT_DEVMEM` block for non-RAM ranges).
- Check: `devmem 0x41200000` before and after load (before load an AXI access
  to unprogrammed PL typically hangs or bus-errors the CPU: **do not read PL
  addresses before G1 passes**).
- If wrong: use the UIO backend (needs a device-tree overlay per IP).
- mmap uses `O_SYNC`; confirm the mapping is device/strongly-ordered memory
  (not cached) so volatile accesses reach the AXI bus in order.

### G5. AXI GPIO driver semantics and the example (VERIFIED 2026-11-16, see Board results)
- `hardroxide-ip::gpio` follows PG144 offsets (DATA 0x0, TRI 0x4, DATA2 0x8,
  TRI2 0xC), `1` = input in TRI. HWH offsets agree (fixture test), but real
  pin behaviour is unverified.
- Check: `cargo run --example gpio_smoke` in a generated crate: 4 LEDs walk a
  one-hot pattern; press buttons/switches and confirm channel 2 reads
  `0b[sw1 sw0 btn3 btn2 btn1 btn0]` as wired by the fixture's `pins.xdc`.
- Fixture pin map (from `fixtures/scripts/build_pynq_z2_gpio.tcl`): LEDs
  R14 P14 N16 M14; buttons D19 D20 L20 L19; switches M20 M19. Verify against
  the PYNQ-Z2 schematic/master XDC; the fixture was built without a board file.

### G6. Fixture fidelity (UNVERIFIED)
- The fixture PS7 is hand-configured (no PYNQ-Z2 board preset): DDR part,
  MIO and peripherals are not board-accurate (the crystal is set to 50 MHz). Since
  loading via Linux only reprograms the PL this should be harmless, but if
  `ps7_init` side effects or the crystal value matter (see G2), rebuild the
  fixture with the real PYNQ-Z2 board files / preset (`tul.com.tw:pynq-z2`)
  and refresh the golden files (`UPDATE_GOLDEN=1 cargo test -p hardroxide-gen`).

### G7. Singleton and process behaviour on the board (VERIFIED 2026-11-16, see Board results)
- The singleton is an in-process `AtomicBool`; it does not stop a second
  *process* (or PYNQ's Python) from loading another bitstream underneath a
  running program. Decide whether to add a lock file (`/run/hardroxide.lock`).
- Check what happens to a running design when the process exits (PL keeps
  running; registers keep state). Document the intended semantic.

### G8. Cross-compilation and deployment (VERIFIED 2026-11-16, see Board results)
- armv7 binaries link with `arm-linux-gnueabihf-gcc` and the runtime tests
  pass under `qemu-arm` (V5). Not verified: that a cross-built binary runs on
  the stock image's glibc (a newer workstation glibc gives `GLIBC_2.xx not
  found`; then build on the board or use `cargo zigbuild` with the board's
  glibc version). The embedded bitstream makes
  binaries ~4 MB.

### G9. HLS IP on hardware (VERIFIED 2026-11-16, see Board results)
- `hardroxide-ip::hls::HlsIp` follows the `ap_ctrl_hs` layout from the
  generated RTL header (CTRL/GIER/IP_IER/IP_ISR, user regs from 0x10).
  `start()` reads CTRL first (to preserve `auto_restart`), which clears the
  read-to-clear `ap_done`/`ap_ready` bits as a side effect, as in Xilinx's own
  driver.
- Check with the vadd fixture: load it, `cargo run --example hls_smoke`
  (reads status only), then in a scratch program `set_a(40)`, `set_b(2)`,
  `start()`, `wait_done(..)`, `out_r() == 42`, also `ap_ready`/`ap_idle`
  transitions and `out_r_ctrl` bit 0 (`ap_vld`).
- Interrupt delivery: `enable_done_interrupt`, wait on `IRQ_INTERRUPT`,
  `ack_interrupts` (see G11 and the [Interrupts](guide/interrupts.md) guide).

- **Pointer arguments** (`fixtures/pynq_z2_hls_scale.xsa`): `set_in_r_buffer`
  / `set_out_r_buffer` write the buffer's physical address into the two halves
  of the 64-bit register pair (checked offline only against the HWH layout).
  On the board, with two `Udmabuf`s: fill `in`, set `n`, `k`, `start()`,
  `wait_done`, then read `out` (expect `in[i] * k`). Also confirm that the HP0
  port reaches the udmabuf's physical range (no AXI decode error) and that
  uncached buffers give the right data without cache maintenance.

### G10. AXI DMA direct mode + udmabuf on hardware (VERIFIED 2026-11-16, see Board results)
- Driver follows PG021 simple mode; register offsets/bits are asserted
  against the HWH (offline), behaviour is not. Mock models write-1-to-clear
  DMASR flags, which is an assumption about the real register semantics.
- Check: load `pynq_z2_dma_loopback`, `insmod u-dma-buf` twice
  (`udmabuf0`, `udmabuf1`, >= 4 KiB each; confirm the module exists on the
  PYNQ image, otherwise build it or use CMA/dma-heap), then `cargo run
  --example dma_loopback`. Expect "DMA loopback OK".
- Things that could differ: udmabuf sysfs paths (`/sys/class/u-dma-buf/...`),
  `O_SYNC` meaning uncached for that module version, buffer physical address
  reachable by HP0 (< 512 MB DDR), S2MM needing TLAST/length semantics (the
  FIFO loopback relies on MM2S asserting TLAST), the S2MM-before-MM2S
  ordering, and `reset()` timing. Also whether the AXI level shifters /
  HP port need enabling (G3).
- `Transfer` halts a channel on timeout/error/drop and leaks the buffer if the
  channel will not halt; confirm the halted state is reached on real hardware.

### G11. UIO interrupts (VERIFIED 2026-11-16, see Board results)
- `hardroxide-irq::UioIrq` implements the standard UIO protocol and is
  tested only against a socketpair. `IrqLine::open` finds `/dev/uioN` by
  scanning `/sys/class/uio` (tested on a fake tree only).
- The generator emits `interrupts.dtso` (one `generic-uio` node per interrupt
  line, `interrupt-parent = <&intc>`, `target-path = "/"`). Verified offline:
  it compiles with `dtc` 1.6.1 and merges onto a Zynq-7000-style base tree with
  `fdtoverlay`, producing the expected `interrupts = <0 29 4>` cells. Not
  verified: that the stock PYNQ kernel/DT accepts it.
- Assumptions to check on the board, in order:
  1. The kernel has `uio_pdrv_genirq` and the command line contains
     `uio_pdrv_genirq.of_id=generic-uio` (otherwise `generic-uio` nodes do not
     bind).
  2. The base DT is built with symbols (`dtc -@`) so `&intc` resolves, and the
     GIC label is really `intc`; otherwise edit the generated overlay.
  3. Overlays can be applied (configfs `/sys/kernel/config/device-tree/overlays`)
     on this kernel, and nodes added under `target-path = "/"` get platform
     devices. If not, put the nodes under the PL bus node (`/amba` or
     `/amba_pl`) instead.
  4. Two nodes may share one register window (an AXI DMA has two interrupts and
     the overlay has two nodes with the same `reg`); the kernel may reject
     overlapping resources. If it does, give the second node no `reg` window
     or use an AXI interrupt controller (now supported, see G13).
  5. The UIO device `name` equals the node name (without `@addr`);
     `find_uio` falls back to the unique `maps/map0/addr`. Check
     `cat /sys/class/uio/uio*/name`.
  6. The IRQ_F2P to SPI mapping (bits 0-7 -> IDs 61-68, 8-15 -> 84-91, DT cell =
     ID - 32; UG585 table 7-4) and the level-high trigger type.
- Run `cargo run --example irq_probe`, then wait on a real event: HLS
  (`enable_done_interrupt`, `start`, `wait`, `ack_interrupts`) and DMA
  (`set_interrupts(true)`, start, `wait`, then `Transfer::wait(ZERO)`), and
  confirm the line re-arms on the second round.

### G13. AXI Interrupt Controller (VERIFIED 2026-11-16, see Board results)
- `hardroxide_ip::intc::AxiIntc` is tested against the mock and a fake
  `IrqWaiter`; register offsets and the `MER` bit positions are asserted against
  the HWH of `fixtures/pynq_z2_intc.xsa`. The fixture builds in Vivado and its
  `interrupts.dtso` compiles and merges like the others.
- To check on the board: with only the controller's UIO node applied, enable
  one source (for example the timer, input 0), wait on `IRQ_IRQ`, and confirm
  `pending()` reports exactly that bit; clear the device, `ack`, and confirm
  the line re-arms. Check `ISR`/`IPR`/`IVR` agree.
- The driver assumes level-sensitive inputs for its ack order guidance; the
  per-input edge/level choice (`C_KIND_OF_INTR`) is not read. Confirm for any
  edge-sensitive source.
- The controller's `irq` sensitivity comes from `C_IRQ_IS_LEVEL`/`C_IRQ_ACTIVE`
  (the HWH marks the port `SIGIS="undef"`); confirm the overlay's trigger cell
  (`4` = level high) matches the hardware behaviour.

### G14. AXI BRAM Controller and UART Lite (VERIFIED 2026-11-16, see Board results)
- Drivers are tested against the mock and a FIFO model; UART register offsets
  and bit positions are asserted against the HWH of `pynq_z2_mem_uart.xsa`.
- Board checks: `cargo run --example bram_test` (whole-RAM pattern, expect 0
  mismatches; also confirm byte writes via `write_bytes` land at the right
  lanes). For the UART, constrain the pins in a real design, loop TX to RX and
  run `uart_hello`; confirm the status bits (`TX_FIFO_Full` at 16 queued bytes,
  `RX_FIFO_Valid_Data`) and that `errors()` clears on read.
- The fixture's UART pins are unconstrained, so it cannot be used on a board
  as is; it only fixes the interface model.

### G15. AXI4-Stream FIFO (VERIFIED 2026-11-16, see Board results)
- `hardroxide_ip::fifo::AxiFifo` is tested against a loopback model; register
  offsets are asserted against the HWH of `pynq_z2_fifo.xsa`. The reset keys
  (`0xA5`), the `TLR`-starts-the-packet order and the `RLR` length mask are from
  PG080 and not checked against hardware.
- Board checks: `cargo run --example fifo_loopback` (37-byte packet; the core returns it rounded up to 40 bytes, so the
  example accepts zero padding). Also: a packet larger than `TDFV` is refused without writing;
  `ISR` flags clear when written with 1s (confirm the write-to-clear sense, the
  driver assumes it); odd lengths (1, 2, 3 bytes) round-trip.
- Only AXI4-Lite register mode is covered, not the AXI4 full data interface.

### G16. The base design owns the PL (FOUND ON HARDWARE)
- The stock-style image loads its own bitstream at boot (`base.bin`) and binds
  drivers to PL addresses: `panther_drm` (HDMI), `panther_audio`, `zocl`,
  `gpio-xilinx` at `0x41200000..0x41250000`, `uio_pdrv_genirq` at `0x43c00000`,
  plus the `panther-desktop` and `jupyter` services.
- Observed: the first `board_check` run froze the whole board (SSH and both
  serial ports silent) in step 1. Reprogramming the PL while those drivers are
  active is the likely cause (a PS to PL access during configuration never
  completes). After stopping them the same load succeeded repeatedly.
- Workaround, run before the first load (as root):
  `systemctl stop panther-desktop jupyter; rmmod panther_drm panther_audio zocl`,
  then unbind the PL platform devices (`echo <dev> > /sys/bus/platform/devices/<dev>/driver/unbind`
  for each `4*` device). Documented in [Running on hardware](guide/hardware.md).
- Guard (implemented): `load_bitstream` now refuses with `FpgaError::PlInUse`,
  naming the devices, when a driver other than `uio_pdrv_genirq` is bound to a
  platform device at a PL address (`FpgaPaths::refuse_bound_pl_drivers`, default
  on). Unit-tested on a fake sysfs tree; its detection on the real board is
  checked in the board results below.

### G17. The PS-PL HP port can wedge after many design switches
Status: **observed, cause unknown.**
- **Observed** on 2026-11-16, twice, on a PYNQ-Z2 (kernel 6.6.10, stock `fpga_manager`),
  after tens of reprogrammings that alternated between designs, including two
  that use an `S_AXI_HP0` master (`pynq_z2_dma_loopback`, `pynq_z2_hls_scale`):
  - transfers through HP0 complete without error but **lose the last 16 words of a
    write** (the receive buffer keeps its old fill value for the final 64 bytes; the
    data never arrives later), and an HLS `m_axi` kernel can time out or write
    shifted data;
  - or the AXI DMA fails its soft reset (`DmaIpError::StuckChannel`) straight after
    `load()`;
  - in that state `AFI_WRDEBUG` (`0xF8008024`) read `2` (one write command in flight
    between the HP port and the memory switch) and `AFI_WRDATAFIFO_LEVEL`
    (`0xF8008020`) read `2` (one stuck 64-bit beat); both read `0` when healthy.
- The state **persisted across reprogramming** and across loading a different design,
  and cleared only by a **power cycle**. A software `reboot` did not bring the board
  back at all in that state.
- **Not the cause** (tried): cache attributes of the buffers (`sync_mode=1`,
  uncached), the HP port width (64-bit in these designs and in the AFI registers),
  holding the PL in reset while programming, aborting a DMA mid-transfer, the DMA
  stop-fallback soft reset, or loading the base overlay.
- **Not reproduced** on a clean boot in a short alternation of the same designs (dozens
  of switches ran clean), and it returned in a long mixed rotation (base overlay,
  interrupt-controller, DMA, HLS pointer and scalar designs) after about twenty
  switches. The register evidence says a PL master vanished in the middle of a write
  burst; the Linux FPGA manager enables the PL-to-PS level shifters while the PL is still
  being configured, which is a plausible way for a spurious beat to enter the AFI
  FIFO, but this was not confirmed.
- **Unrelated freeze found while investigating:** reading the AFI FIFO-level registers
  when the HP port clock is not running hangs the CPU (the register documentation says
  so). Do not read them unless the design clocks the HP port. Hardroxide never does.
- **Advice:** treat it as a property of repeated PL reprogramming on this platform.
  Reprogram as seldom as practical (one `Design::load()` per process, keep one design
  across runs), and if DMA or `m_axi` results look wrong, or `StuckChannel` appears
  right after load, power-cycle the board before debugging your logic. The DMA
  driver's error message says so.
- **Next:** a stress script around `fixtures/board_tests/` that switches designs and
  checks a known transfer each time would find a trigger; a library-level mitigation
  needs the actual cause.

## Design gaps (implemented partially or not at all)

- **D1 Interrupts**: traced and wired into the generated crate (`IrqLine`
  constants, `interrupts.dtso`, `irq_probe`), including fan-in through an
  `axi_intc` (`via_intc`, `AxiIntc`); see G11 and G13 for what is unverified.
  Not done: interrupts through
  logic other than `xlconcat`/direct, an overlay variant targeting the PL bus
  node, and PS-side GPIO/other interrupt sources.
- **D2 Other IP**: curated drivers exist for AXI GPIO, DMA (direct mode),
  Timer, IIC, Quad SPI (standard mode), interrupt controller, BRAM Controller,
  UART Lite and AXI4-Stream FIFO (AXI4-Lite mode). HLS IP gets generated typed
  accessors for 32-bit scalar registers and 64-bit pointer register pairs
  (`set_*_addr` / `set_*_buffer`, no size check against the buffer). Not done:
  DMA scatter-gather, 64-bit DMA addressing, HLS AXI-Stream and array (BRAM)
  ports, `ap_ctrl_chain` specifics, Timer 1 / PWM / capture, SPI dual/quad
  modes. Any other IP (custom RTL included) is exposed as a raw register
  window with its `pac` constants.
- **D3 Multiple address blocks / segments**: the PAC emits only the first
  address range of an instance and the registers of its matching block.
- **D4 `hardroxide update` / `dev-bitstream`** (milestone 2): implemented and
  tested offline (bitstream-only update touches exactly `bitstream/design.bin`
  and `design.sha256`; interface change prints a line diff and needs
  `--regen`; `Design::load_with_bitstream` checks the XSA interface hash).
  Gaps: the diff is a line-set diff of the canonical interface text, not
  grouped by IP; `--regen` keeps the existing `Cargo.toml` and does not
  accept a new dependency path; the interface hash is compared only at update
  and `load_with_bitstream`, never against the running hardware. Not exercised
  on the board (G1 prerequisite).
- **D5 Other Vivado versions / designs**: HWH parsing is validated against
  eight 2025.2 fixtures plus real third-party designs (the PYNQ `base`,
  `logictools` and six `vidcap` overlays, Vivado 2024.1 to 2025.2), an HLS design
  from Vivado 2019.2 and an UltraScale+ RFSoC design from Vivado 2020.2 (parsed;
  `generate` refuses it with `UnsupportedArch`). Only parsing and generation are
  checked for versions other than 2025.2; they have not been run on hardware.
  Still missing: Vivado 2021 to 2023 samples and UltraScale+ support itself.
- **D6 `cargo hardroxide` shim, optional zstd, rename map, async**: not done.
- **D7 UIO backend**: `Uio` maps map 0 only; untested against a real device.
- **D8 CI**: `.github/workflows/ci.yml` runs fmt, clippy, all tests (including
  the `fdtoverlay` merge and the compile-tested guide examples), the mdBook
  build, and the armv7 tests under `qemu-arm`. Runs green on GitHub.
- **D9 Bitstream-only updates and examples**: `hardroxide update` replaces only
  the bitstream files; it does not regenerate `examples/`, which also read
  some IP parameters (for example the GPIO width in `gpio_smoke`). Those are
  not part of the interface hash, so after such an update an example may be
  stale (driver constructors are covered, see `driver_config`). Rerun with
  `--regen` to refresh them.
