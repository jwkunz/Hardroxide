# What each library can and cannot do

One page for the whole project: for each crate, what it is for, what it does, what it does not, and how much has been checked and where. "Board" means a PYNQ-Z2 (Zynq-7000), "QEMU" its emulated `xilinx-zynq-a9`
machine, "host" an ordinary `cargo test`. The details are in each crate's own reference page; the verification results are in [Verification gaps](../08-verification-gaps.md).

## The generator and the host tools

| Crate | For | Can | Cannot | Checked |
|---|---|---|---|---|
| [`hardroxide-hwh`](hwh.md) | reading a Vivado XSA (the bitstream and the hardware handoff) | instances, address maps, clocks, interrupts, the register maps of IP, the PS7 configuration; hashes of the interface and the bitstream | Zynq UltraScale+ PS settings beyond clocks, console and interrupts; more than one address block per instance; HWH files from Vivado before 2024 (not seen) | host: golden snapshots of the fixture designs; board: PYNQ's own base overlay; (3.0) the RFSoC 4x2 designs and its base overlay |
| [`hardroxide-gen`](gen.md) | `hardroxide generate`: a typed Rust crate from an XSA, for Linux, bare metal or the RTOS | a `Design` with a typed driver per IP, register constants, the embedded bitstream, interrupt tables, a linker script and examples; `update` swaps a bitstream without regenerating when the interface is unchanged | custom IP beyond what the register map says (HLS stream and array ports, scatter-gather DMA); partial reconfiguration; Python bindings; (3.0, so far) Linux crates for Zynq UltraScale+ | host: golden snapshots, compile tests of the generated crates, guide examples; board: all three flavors on the PYNQ-Z2, bare metal and RTOS on the RFSoC 4x2 |
| `hardroxide-cli` | the `hardroxide` binary: `generate`, `update`, `inspect`, `run`, `package`, `make-cert`, `doctor` | drive a board over JTAG with the Vitis tools, build a `BOOT.BIN`, make a self-signed certificate | run without the Vitis tools (`run`, `package`, `doctor` say so and stop); `make-cert` needs a Unix host (`/dev/urandom`) | host tests; board: `run` on every example; `make-cert` against OpenSSL and curl |

## The runtime

| Crate | For | Can | Cannot | Checked |
|---|---|---|---|---|
| [`hardroxide-mmio`](mmio.md) | memory-mapped registers and block devices behind one trait | `/dev/mem`, UIO, physical (bare metal) and mock backends; `BlockDevice`; a time source for timeouts | cached device memory (device and DMA memory is uncached) | host, armv7 under `qemu-arm`, board |
| [`hardroxide-fpga`](fpga.md) | programming the PL and setting its clocks | load a bitstream and set FCLK from the HWH, on Linux | partial reconfiguration; UltraScale+ | board (Linux) |
| [`hardroxide-dma`](dma.md) | DMA buffers | contiguous uncached buffers behind the `DmaBuffer` trait | cached buffers with explicit cache maintenance against a PL master; scatter-gather | board: loopback and speed tests |
| [`hardroxide-irq`](irq.md) | PL interrupts | UIO-based waiting (sync, and async behind a feature); an `IrqWaiter` trait the drivers use | none beyond the platform's own limits (one interrupt per UIO device; trigger type is fixed per boot) | board (Linux and bare metal) |
| [`hardroxide-ip`](ip.md) | drivers for Xilinx IP, written against `Mmio` | GPIO, HLS blocks, AXI DMA (direct register mode), AXI Timer, IIC, Quad SPI, BRAM, UART Lite, FIFO, interrupt controller, the PYNQ audio controller, VDMA (read channel), the video timing controller, the dynamic clock; `embedded-hal` 1.0 on the timer, IIC and SPI | the VDMA write channel (HDMI input), SPI with other than standard mode, DMA scatter-gather, any IP not in the list | host mocks for all; board for the ones the base overlay and the test designs use (SPI against a real device was not run) |

## The processor and the kernel

| Crate | For | Can | Cannot | Checked |
|---|---|---|---|---|
| [`hardroxide-ps7`](ps7.md) | the Zynq's processing system on bare metal | start-up, exceptions, GIC, private and global timers (including a periodic comparator interrupt), UART console, MMU and caches (`set_non_cacheable` for DMA windows), I2C, SPI, the SD host controller, the Gigabit Ethernet MAC | the second core (SMP); USB, CAN and QSPI controllers; (SPI is new in 3.0 and was run on UltraScale+ only); the second SD controller and MAC were not run; DMA for the SD controller (programmed I/O only); interrupts for the MAC (polled) | host mocks; QEMU (start-up, timers, GIC, SD, Ethernet); board (all of the above on one card and one cable) |
| [`hardroxide-psu`](psu.md) | the processing system of Zynq UltraScale+ on bare metal (3.0) | start-up at EL3, exceptions, GIC, generic timer, clocks, MMU and caches, console, GPIO, die temperatures; re-exports the I2C, SPI, SD and Ethernet drivers | the other cores; other exception levels; USB, DisplayPort, PCIe | host mocks; QEMU; RFSoC 4x2 |
| [`hardroxide-board`](board.md) | chips on FPGA boards, over the `embedded-hal` traits | INA220 power monitors, the AT24MAC402 identity EEPROM (read only), the US2066 OLED, LMK04828 and LMX2594 clock chips with lock readout; the RFSoC 4x2's wiring | other chips; configuring the monitors or the synthesizers beyond their register files | host mocks; RFSoC 4x2 |
| [`hardroxide-rt`](rt.md) | a small preemptive kernel for core 0 | tasks (up to 16), priorities with inheritance, semaphores, mutexes, queues, a periodic timer, sleeping on a hardware interrupt, shared access to the `Design`, per-task statistics; Cortex-A9 and (3.0) Cortex-A53 | event flags, software timers, task suspend and delete from another task, GIC-priority-mask critical sections, a tickless idle, both cores | host (the scheduler); QEMU; board: a 2-hour run with a 10 ms task released at most 10.2 us late over 726,197 releases, and the integrated application |

## Applications on the base overlay

| Crate | For | Can | Cannot | Checked |
|---|---|---|---|---|
| [`hardroxide-video`](video.md) | the HDMI output | modes from 640x480 to 1920x1080 at 60 Hz, double buffering, the whole bring-up | HDMI input; 640x480 was refused by the tested monitor; no audio over HDMI | board: 800x600, 1024x768, 1280x720, 1280x1024, 1920x1080 on a monitor |
| [`hardroxide-gfx`](gfx.md) | drawing on a pixel buffer | lines, rectangles, circles, triangles, blits with colour key and alpha, a 5x7 font, BMP reading and writing | anti-aliasing, fonts other than the built-in one, anything beyond 32-bit `0x00RRGGBB` pixels | host: rendered pictures compared with expected ones; board: the demos |
| [`hardroxide-audio`](audio.md) | the codec and sound | the ADAU1761 over I2C, WAV parsing, tones, resampling, polled playback and recording, an interrupt-driven pump | the microphone input was not heard; sample rates other than the 48.8 kHz of the controller (resampled in software); compressed formats | board: tones, a WAV, a loopback with a cable, recording to memory, the interrupt pump with no underruns; **not run: listening to the pump** |
| [`hardroxide-fat`](fat.md) | FAT32 on a block device | mount (MBR or bare), long names, directories, read, write, append, seek, truncate, remove, format | FAT12/16, exFAT, rename, files over 4 GiB, a second partition | host: `mkfs.fat` images, `fsck.fat`; QEMU; board: a 30 GiB card formatted and filled; **a board-formatted card has not been through `fsck.fat` on a PC** |
| [`hardroxide-net`](net.md) | a small IPv4 stack | Ethernet, ARP, ICMP echo, UDP, a DHCP client, TCP (several connections, retransmission, windows), an HTTP/1.1 server | IPv6; fragmentation; DNS; congestion control and out-of-order buffering; HTTP keep-alive | host: against itself over a lossy wire and 30,000 random frames; QEMU against curl; board against curl: 3,200 requests/s |
| [`hardroxide-crypto`](crypto.md) | the primitives of TLS | SHA-256/512, HMAC, HKDF, ChaCha20-Poly1305, X25519, Ed25519, ECDSA P-256 with RFC 6979, a bit generator | AES, RSA, other curves; **a source of randomness** (the Zynq has none); anything audited | host: more than 800 vectors against OpenSSL, byte for byte for P-256 signatures; **not audited; constant time is by construction, not measured** |
| [`hardroxide-tls`](tls.md) | a TLS 1.3 server and HTTPS | one cipher suite, X25519, ECDSA P-256 or Ed25519 certificates; the HTTP server over TLS | client certificates, resumption, 0-RTT, HelloRetryRequest, KeyUpdate, TLS 1.2, certificate chains | host and QEMU against curl and OpenSSL; board against curl, OpenSSL and Edge; **not audited; the random bytes are weak** |

## Reading the table

*Checked* means what was run, not what is believed: the [Verification gaps](../08-verification-gaps.md) page lists every board result with its numbers and, for each, what was **not** run. Two things to keep in mind when choosing what to rely on:
the network and TLS code is home-made and has had no outside review, and the random bytes for TLS are weak (see the [TLS reference](tls.md#where-the-random-bytes-come-from)); everything else was checked against an independent implementation or a real device, but on one
board, one card, one cable and one PC.
