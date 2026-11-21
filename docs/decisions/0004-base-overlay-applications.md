# ADR 0004: Applications for the PYNQ-Z2 base overlay (2.1 to 2.7)

Status: **accepted** 2026-11-21 (the decisions under "Decided" are the maintainer's; the version plan may be revised as work
proceeds). Builds on [ADR 0003](0003-bare-metal-and-rtos-platforms.md).

## Goal

2.0 gave the bare-metal and RTOS flavors a kernel, drivers for the PS and the generic IP, and examples that blink LEDs and
move bytes. Nobody buys a board for that. The 2.x series shows what the flavors are *for*, on the design every PYNQ-Z2
user already has: the PYNQ **base overlay** (`fixtures/real/pynq_base_overlay.xsa`). Each minor version adds one
reusable library and the example applications that prove it, in a bare-metal and an RTOS variant:

| Version | Theme | Library | Showcase |
|---|---|---|---|
| 2.1 | Base overlay foundation | none (drivers and tooling) | buttons, switches, LEDs, RGB LEDs, interrupt-driven; the whole base overlay brought up bare metal and RTOS |
| 2.2 | Pixels | `hardroxide-video`, `hardroxide-gfx` | a pixel-banging graphics library: framebuffer on the HDMI output, drawing primitives, text, animation |
| 2.3 | Sound | `hardroxide-audio` | WAV playback, tone synthesis, recording; line-in to line-out |
| 2.4 | Storage | `hardroxide-sd`, `hardroxide-fat` | an SD card and FAT32 reader/writer; a WAV player and an image viewer that read from the card |
| 2.5 | Network | `hardroxide-net` | Ethernet: ping, UDP, DHCP, TCP and a web server that controls the board |
| 2.6 | Secure web | `hardroxide-crypto`, `hardroxide-tls` | HTTPS with a self-signed certificate: the 2.5 web server over TLS 1.3 |
| 2.7 | Integration | none | one RTOS application that serves a web page, plays audio and draws a display together; hardening and a documentation pass |

Every version is a **minor** increment: it adds crates, drivers and examples, and changes existing public APIs only
additively (SemVer). The 2.0 generated-crate interface stays as it is.

## Principles

1. **Same rules as 2.0.** No third-party dependencies in the new crates (only `embedded-hal` and `thiserror` where a
   public trait or error needs them), `no_std`, `unsafe` minimised and documented, deterministic generation, docs in the
   book updated with every change (reference page, tutorial, verification row).
2. **Layering.** Drivers for PS peripherals (Ethernet MAC, SD host, PS I2C) live in `hardroxide-ps7`. Drivers for base
   overlay IP (VDMA, video timing controller, pixel clock, audio codec) live in `hardroxide-ip` next to the existing ones,
   keyed by VLNV, and are used by the generated `Design` like any other driver. The libraries above them (`gfx`, `audio`,
   `fat`, `net`) know nothing about the board: they work on byte buffers, sample buffers and block devices, so they are
   tested on the host and under QEMU, and reused by any design.
3. **Host-testable cores.** Parsers, protocol state machines, the drawing code and the file system are pure code with unit
   tests on the host (golden images, captured packets, a FAT image built in the test). Only the thin hardware layer needs a
   board.
4. **Both flavors, one code base.** A library is written once and used from a bare-metal `main` (a polling loop) and from
   RTOS tasks (one task per activity, `Queue`/`Shared` between them). Each application ships both variants, and the RTOS
   variant is where the kernel's priorities earn their keep (audio at a hard deadline, network in the background).
5. **Verified on the board before it ships.** Each version has acceptance tests that run on a PYNQ-Z2 and are recorded
   in [Verification gaps](../08-verification-gaps.md), including what was *not* tested.
6. **Applications are examples, not products.** They live in their own workspace (`baremetal/apps/`, like `rt-smoke`),
   are compiled by CI, documented as tutorials in a new "Applications" tutorial group, and are not published.

## What the base overlay gives us

`hardroxide inspect fixtures/real/pynq_base_overlay.xsa` shows the instances reachable from the processor (the four
MicroBlaze subsystems are private to their cores and get no field):

| Need | Base overlay instances | Today in Hardroxide |
|---|---|---|
| Buttons, switches, LEDs | `btns_gpio`, `switches_gpio`, `leds_gpio`, `rgbleds_gpio` (AXI GPIO) | curated driver |
| Interrupts | `system_interrupts` (AXI INTC) into one PL line | curated driver |
| HDMI output | `video_axi_vdma`, `video_hdmi_out_frontend_vtc_out` (v_tc), `..._axi_dynclk` (Digilent pixel clock), `video_hdmi_out_color_convert` and `..._pixel_unpack` (HLS) | VDMA, v_tc and dynclk are **raw register windows**; HLS wrappers generated |
| HDMI input | `video_hdmi_in_*` (v_tc, pixel_pack, color_convert, GPIO), VDMA write channel | same; out of scope until 2.7 |
| Audio | `audio_codec_ctrl_0` (a custom core, AXI-Lite only, I2S pins `bclk`, `lrclk`, `sdata_i/o`) | raw window; **no register map in the HWH** |
| SD card | PS SDIO 0 on MIO 40 to 45 | none |
| Ethernet | PS GEM 0 on MIO 16 to 27, RGMII, 1000 Mbps | none |
| Console | PS UART 0 on MIO 14/15 | `hardroxide-ps7` |

Facts that shape the plan:

- **No `ps7_init` in the base XSA.** The fixture is the overlay's `.bit` and `.hwh` repackaged, so `hardroxide run`
  (which takes `ps7_init.tcl` from the XSA) cannot bring the board up from it. The HWH does carry the full PS
  configuration (`PCW_*`: DDR, MIO, clocks, GEM, SDIO, UART), so 2.1 produces a `ps7_init` for it (see 2.1).
- **Audio is programmed I/O.** `audio_codec_ctrl_0` has no stream port and no interrupt, so the processor feeds samples
  through registers at the sample rate. That is a real-time problem: a polling loop on bare metal, a high-priority task
  or a timer interrupt on the kernel. It is also the best demonstration of what the RTOS is for.
- **Ethernet and SD are PS peripherals**, independent of the PL. They need the PS clocks `ps7_init` programs (IO PLL
  divisors for the GEM and the SD host), which is another reason the init story comes first.
- **A framebuffer is large.** 1280x720 at 32 bits is 3.5 MiB, and double buffering doubles it. `hardroxide-ps7`'s
  non-cacheable `.dma` section is 1 MiB today; 2.2 makes its size configurable, and uses cached buffers with explicit
  cache maintenance (`clean_range` after drawing), which 2.0 provides but has not yet exercised against a PL master.
- **Licensing.** The PYNQ base overlay is Apache-2.0 and already in the repository with attribution. Drivers are written
  from the datasheets and the vendor's register documentation (numbers only, as for 2.0); where PYNQ's open-source drivers
  are the only description of a custom core (the audio codec controller), they are read as a specification of
  register offsets, the code is not copied, and the source is credited in the reference page.

## Plan by version

### 2.1: Base overlay foundation

The work that every later version stands on.

- **Bring-up path for the base overlay:** a Vivado script that builds a hardware-only XSA from the PS configuration in the
  base HWH (so `ps7_init.tcl` matches exactly), and `hardroxide run --ps7-init <file>` (or an XSA that carries it) to run
  the repackaged base XSA with it. The result is committed under `fixtures/real/` with its script.
- `hardroxide generate --platform baremetal|rtos` on the base overlay builds, `board_check` passes, and a regression
  test generates and builds it for both platforms.
- Examples (bare metal and RTOS): buttons to LEDs, the RGB LEDs as a mood light, switches choosing a pattern, all
  through `system_interrupts` (buttons wake a task instead of being polled).
- `hardroxide-ps7`: nothing new expected; this version finds what is missing.
- Docs: a new "Applications" tutorial group with the first tutorial; reference page for the base overlay.
- **Acceptance:** from a clean checkout, `hardroxide run` starts the RTOS mood-light application on the base overlay;
  button presses reach a task through the interrupt controller with measured latency.

### 2.2: Pixels (HDMI output, a graphics library)

- **Drivers in `hardroxide-ip`:** `AxiVdma` (frame buffer addresses, stride, genlock/park, interrupts), `VideoTimingController`
  (the timing generator for a chosen mode), `AxiDynclk` (the MMCM reconfiguration for a pixel clock). Curated drivers
  are keyed by VLNV, so the generator picks them up and the base overlay's raw windows become typed fields.
- **`hardroxide-video`:** display bring-up for 640x480@60, 1280x720@60 (and 1920x1080 only if the design's datapath
  and bandwidth allow; measured, not assumed); pixel formats; front/back buffer flip on the vertical-blank interrupt.
- **`hardroxide-gfx`:** a small 2D library over a `&mut [u32]`-like surface: pixels, lines, rectangles, circles, triangles,
  blits and an alpha blend, a built-in bitmap font and text, double-buffered animation. Written for this project; the font
  is public domain or drawn in-repo, with its provenance recorded. Pure code, host-tested against golden images
  (rendered to a PNG-comparable buffer in the tests).
- `hardroxide-ps7`: configurable `.dma` section size; a cached-buffer path with `clean_range`.
- Examples: test pattern (colour bars, gradients), a plasma/bouncing-sprite animation at 60 frames per second, a text
  console on screen, and an RTOS version with a render task, an input task and an on-screen frame-time display.
- **Acceptance:** a stable 60 Hz image on a monitor, a double-buffered animation with no tearing, frame-time statistics
  recorded; the DMA/cache behaviour (cached versus non-cacheable framebuffer) measured and written down. Risk: the
  documented HP-port wedge ([G17](../08-verification-gaps.md)) may show up with sustained VDMA traffic.

### 2.3: Sound (audio playback and recording)

- **Discover the codec controller.** The register map of `audio_codec_ctrl_0` is not in the HWH. It is recovered from
  PYNQ's open-source audio driver (as a specification), confirmed on the board with a loopback, and documented in the
  reference page. If the ADAU1761 codec itself needs I2C configuration, the PS I2C controller gets a driver in
  `hardroxide-ps7` (the base design enables I2C 0).
- **Driver in `hardroxide-ip`:** `AudioCodec` (sample rate, volume, input select, push/pull samples).
- **`hardroxide-audio`:** a WAV (RIFF PCM) parser and writer, sample-format conversion (8/16/24-bit, mono/stereo, sample
  rate), a ring buffer, a tone and waveform generator, a simple mixer, and a `Player`/`Recorder` over a trait for the
  codec so the logic is host-tested with a fake one.
- Examples: play a WAV embedded in the binary (`include_bytes!`); a synthesiser (the buttons play notes); record N
  seconds into DDR and play it back; line-in to line-out passthrough. RTOS variants feed the codec from a
  high-priority task released at the sample-period grid, with the control and display tasks running beside it; the 2.0
  jitter benchmark's histograms are reused to show the audio task never misses a deadline.
- **Acceptance:** an audible, glitch-free 48 kHz stereo WAV for ten minutes while the other tasks run, with underrun
  counters at zero; a recording whose spectrum shows the test tone. Saving a recording to a card waits for 2.4.

### 2.4: Storage (SD card, FAT32)

- **`hardroxide-ps7`:** the SD host controller (SDIO 0) driver: card initialisation, block read and write (programmed I/O
  first, then ADMA), card detect, clock speeds; implements a `BlockDevice` trait.
- **`hardroxide-fat`:** a FAT32 file system over `BlockDevice`: read first (directory walk, long names for reading,
  file reads), then write (create, append, delete), with a write-back sector cache. Tested on the host against FAT images
  built in the test and checked with the host's `fsck.fat`, so the format is right independently of our own reader.
- Examples: list and print files; an SD WAV player (the 2.3 `Player` reading a file from the card); a BMP/raw-image viewer
  onto the 2.2 framebuffer; a data logger that writes sensor readings (switches, a timer) to a file; recording to a
  `.wav` on the card.
- **Acceptance:** read and write a 1 GiB-class card with data integrity checks (file CRCs compared with the host);
  hot-card-absent and bad-card errors are reported, not hung on.

### 2.5: Network (Ethernet, a web server)

- **`hardroxide-ps7`:** the Gigabit Ethernet MAC (GEM 0) driver: descriptor rings in non-cacheable memory, receive and
  transmit, MDIO and the PHY (link state, speed and duplex negotiation), interrupts.
- **`hardroxide-net`:** a small IPv4 stack written for this project: Ethernet, ARP, IPv4, ICMP echo, UDP, a DHCP client with
  a static fallback, TCP (retransmission, a window, several connections) and an HTTP/1.1 server with a routing table and
  a form/JSON helper. Protocol state machines are pure and host-tested with captured and synthesised packets, and the
  whole stack runs against a virtual interface in tests.
- Examples: answer `ping`; a UDP echo and a UDP log sink; a **bare-metal web server** (a status page, buttons and
  LEDs controlled from a browser, the 2.2 framebuffer served as an image); the RTOS variant, where the network is one
  task among several and the control loop keeps its 10 ms deadline while the server is loaded.
- **Acceptance:** `ping` and `curl` from the host; a page-load and request-per-second benchmark; the 10 ms loop's
  jitter measured again with network load. Risks: TCP is the largest single piece of new code; the
  PHY's behaviour at gigabit needs checking on the real board.

### 2.6: Secure web (TLS with a self-signed certificate)

A browser warns about a self-signed certificate, but the connection is encrypted, which is what a board on a home or lab
network needs for a login form or a control page. Scope: **TLS 1.3 server only**, one cipher suite, one key exchange.

- **`hardroxide-crypto`:** the primitives, written for this project, constant-time where the secret allows it, and tested
  against the published test vectors (RFC 7539 for ChaCha20-Poly1305, RFC 7748 for X25519, RFC 8032 for Ed25519, FIPS 180
  and RFC 4231 for SHA-256 and HMAC, RFC 5869 for HKDF), plus differential tests against the host's OpenSSL. Initial set:
  SHA-256, HMAC, HKDF, ChaCha20-Poly1305, X25519, Ed25519. (AES-GCM only if a browser needs it; every current browser
  offers ChaCha20-Poly1305.)
- **`hardroxide-tls`:** the TLS 1.3 server handshake and record layer over a byte stream (the 2.5 TCP connection), X.509
  parsing only as far as sending the certificate. No client authentication, no renegotiation (TLS 1.3 has none), no session
  resumption in the first cut. The HTTP server from 2.5 runs over it unchanged.
- **The certificate:** generated on the host (a `hardroxide` subcommand writing a self-signed Ed25519 certificate and key,
  checked against OpenSSL) and embedded in the program. Generating it on the board needs a source of randomness the Zynq
  does not provide by itself, so it is a later option.
- **Randomness.** The handshake needs unpredictable random numbers. The first cut seeds a generator from the
  jitter of independent clocks and a stored seed; whether the base overlay's XADC (if instantiated) can serve as a better
  source is investigated first. The weaknesses of the final design are written in the reference page.
- **Security statement.** Home-made cryptography is a risk, and this is an educational stack: the documentation says so, lists
  what was and was not reviewed (no independent audit), and recommends it for a trusted network, not the open internet.
  Test vectors, fuzzing of the parsers on the host, and the differential tests are the minimum before it ships.
- **Acceptance:** `curl --insecure` and a browser (after accepting the certificate) load the page; `openssl s_client`
  completes the handshake and reports TLS 1.3; handshake time and requests per second measured; the 10 ms control loop's
  jitter measured with TLS load. Risks: the handshake's public-key operations are slow on a 650 MHz core without
  assembly (measured, expected to be a fraction of a second per new connection); this is the largest security-sensitive piece
  of the project.

### 2.7: Integration and hardening

- One RTOS application that does it all: a web page (over TLS) controls an on-screen graphics demo and an audio player reading
  from the card, while a 10 ms control task keeps running; per-subsystem load and a task monitor page.
- HDMI input capture to the framebuffer (the other half of the base overlay's video path), if 2.2 left room.
- Long-run soak (the 2.0 benchmark's method), a pass over the documentation and tutorials, and a consolidated
  "what each library can and cannot do" reference.

## Cross-cutting work in every version

| Item | How |
|---|---|
| Tests | host unit tests for every library; QEMU for the drivers whose devices QEMU models (its Zynq machine has a GEM and an SD host, to be confirmed for the version in `scripts/test-qemu.sh`); board acceptance tests recorded in [Verification gaps](../08-verification-gaps.md) |
| CI | the applications workspace builds for `armv7a-none-eabihf` in CI; the libraries' host tests run in `cargo test --workspace` |
| Docs | a reference page per library and driver, a tutorial per application under "Applications" (bare metal and RTOS variants side by side), the ADR phase table, the changelog; `book.rs` guard tests keep signatures honest |
| Board regression | start a scripted board regression (the roadmap's "automated board regression script") with 2.1, growing each version: it runs each application's self-test over JTAG and checks the console output |
| Releases | each minor version is a tagged release with its own verification rows; nothing ships on "builds but untested" |

## Hardware needed from the maintainer

| Version | Needed |
|---|---|
| 2.1 | the PYNQ-Z2 (have) |
| 2.2 | an HDMI monitor and cable on the board's HDMI OUT |
| 2.3 | headphones or speakers on the line/headphone output; a source with a 3.5 mm cable for line-in |
| 2.4 | a microSD card (and a reader on the host to prepare it) |
| 2.5, 2.6 | an Ethernet cable to a network or directly to the host; a way to see the board's address (the console, a DHCP lease list) |

## Risks

| Risk | Mitigation |
|---|---|
| TCP/IP is a lot of code to get right | host-tested state machines and a virtual interface; scope TCP to what an HTTP server needs first (a few connections, no options beyond MSS/window scaling off) |
| Home-made cryptography | test vectors, differential tests against OpenSSL, parser fuzzing, one cipher suite, a plain security statement; recommended for trusted networks only |
| The audio core's register map is undocumented | recover it from PYNQ's driver, verify on the board with a loopback before building on it; the project can still ship 2.3 later than 2.4 if it stalls |
| The HP-port wedge (G17) under sustained VDMA traffic | 2.2 measures it first, on a bare-metal run with no kernel in the way, which is exactly the experiment ADR 0003 predicted |
| Non-cacheable framebuffers are slow for the CPU | 2.2 implements cached buffers with explicit cache maintenance and measures both |
| Scope creep (USB, camera, wireless) | out of scope for 2.x; they would be a later ADR |
| `ps7_init` for a design we did not build | 2.1 produces it from the HWH's own PS settings and checks that the board boots with it |

## Decided

1. **The TCP/IP stack is our own** (maintainer, 2026-11-21): no `smoltcp`. TLS is planned the same way (2.6), with
   self-signed certificates; its cryptography is also written here, under the test and disclosure rules above.
2. **Order as proposed** (video, audio, storage, network, secure web, integration).
3. **Hardware:** the maintainer has all of it and will connect each item when its version needs it: the HDMI monitor for
   2.2, audio cables for 2.3, a microSD card for 2.4, Ethernet for 2.5 and 2.6.
4. **Publishing is deferred.** Nothing in 2.x is published for now; the libraries are published later, once the maintainer
   decides, after their board acceptance tests pass.

## Documentation obligations

Each version updates: the reference pages (new crates and drivers), the "Applications" tutorials (output captured from a
board), [Verification gaps](../08-verification-gaps.md) (what passed, what was not tested), the roadmap, the changelog and
this ADR's status table below.

## Status

| Version | State |
|---|---|
| 2.1 | not started |
| 2.2 | not started |
| 2.3 | not started |
| 2.4 | not started |
| 2.5 | not started |
| 2.6 | not started |
| 2.7 | not started |
