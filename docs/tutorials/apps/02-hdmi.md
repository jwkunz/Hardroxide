# Applications 2: Pixels on the HDMI output

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** bare metal and RTOS (the real-time kernel)

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 1](01-base-lights.md), an HDMI monitor on the board's **HDMI OUT**

> **Status: run on a PYNQ-Z2 and a monitor (2026-11-21).** 800x600, 1024x768, 1280x720, 1280x1024 and 1920x1080 work; 640x480 is rejected by the tested monitor (open item).
> The first run did not: the monitor saw nothing. The findings are in section 7, and are why the code is the way it is ([Verification gaps](../../08-verification-gaps.md)).

The base overlay's HDMI output is a pipeline of five pieces: frame buffers in memory, a VDMA that reads them out as a video stream, a pixel
unpacker and a colour converter, and the HDMI transmitter, paced by a video timing generator and a pixel clock whose frequency software sets.
`hardroxide-video` brings all of it up for a display mode; `hardroxide-gfx` draws into the frame buffers.

## 1. What is new

| Piece | Where | What it does |
|---|---|---|
| `AxiVdma`, `VideoTimingController`, `AxiDynclk` | [`hardroxide-ip`](../../reference/ip.md) | typed drivers; the generated `Design` now has them as fields instead of raw windows |
| `Display`, `VideoMode` | [`hardroxide-video`](../../reference/video.md) | bring-up for 640x480 to 1920x1080 at 60 Hz, double buffering |
| `Surface`, `Color` | [`hardroxide-gfx`](../../reference/gfx.md) | pixels, lines, rectangles, circles, triangles, blits, text |

## 2. Build the bare-metal demo

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal -o work/base --hardroxide-path . --name design_1
$ cd work/base
$ cargo add --dev --path ../../crates/hardroxide-gfx
$ cargo add --dev --path ../../crates/hardroxide-video --no-default-features
$ cp ../../apps/base/baremetal/video_demo.rs examples/
$ cargo build --release --example video_demo
$ hardroxide run target/armv7a-none-eabihf/release/examples/video_demo --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 30
```

The program draws a test pattern into one frame buffer, starts the output in 1280x720, and shows the pattern for five seconds, then animates a
bouncing ball for ten with double buffering. On the console:

```console
== video_demo: 1280x720@60 ==
pixel clock 74285600 Hz (asked 74250000)
showing the test pattern for 5 s
animating for 10 s
601 animated frames in 10010 ms = 60 frames per second; display errors: false
```

On the monitor: eight colour bars, a hue gradient and a gray ramp, a grid, and the text HARDROXIDE for five seconds, then a hue-cycling ball bouncing over the bars at 60 frames per second
(the program then stops and the display holds the last frame, so the picture freezes: that is the program having ended).

## 3. The two frame buffers

```rust,ignore
let mut front = static_dma_buffer!(FRAME_BYTES).unwrap();     // 1280 x 720 x 4 bytes, non-cacheable
let mut back  = static_dma_buffer!(FRAME_BYTES).unwrap();
let frames = [front.phys_addr() as u32, back.phys_addr() as u32];
...
let mut s = Surface::new(as_pixels(front.as_mut_slice()).unwrap(), 1280, 720);
s.fill_circle(x, y, 40, Color::hue(h));
```

The frame buffers are at fixed addresses in the window of DDR (`0x1000_0000` to `0x1FFF_FFFF`) that this overlay's video DMA can reach, made non-cacheable with `ps7::mem::set_non_cacheable`, so the CPU's drawing and the
VDMA's reads see the same bytes with no cache maintenance. (A buffer anywhere else, such as from `static_dma_buffer!`, ends in a DMA decode error and a blank screen: see section 7.) The cost is that drawing goes
straight to DDR; at 720p a full-screen redraw is 3.5 MiB of writes, so the demo paints the pattern once and repaints only the small area around the ball each frame. A cached buffer with `clean_range` before each flip is the faster variant and is future work.

## 4. Double buffering

The VDMA runs in park mode: it repeats the frame buffer it is parked on. Switching is one register write, `display.show(n)`, which takes
effect at the next frame boundary:

```rust,ignore
let hidden = if showing == 0 { &mut back } else { &mut front };
// ... draw into `hidden` ...
showing ^= 1;
display.show(showing)?;                 // from the next frame on, the new buffer is scanned out
display.wait_frame(Duration::from_millis(100))?;   // the switch has happened; the old buffer is free
```

## 5. Under the kernel: pacing by the display's interrupt

`apps/base/rtos/rt_video.rs` does the same on `--platform rtos`. A render task (priority 5) sleeps on an `IrqEvent` for the VDMA's frame-done
interrupt (input 1 of the interrupt controller), so it runs exactly once per frame without polling; a priority-3 task burns 2 ms of every 10 ms, and a
priority-2 task prints the worst drawing time and the worst deviation of the frame interval from 16.667 ms every 5 seconds. The kernel's job here is
to keep the frame interval steady however busy the rest is. Build it as in section 2 with the RTOS crate (`--platform rtos`):

```console
== rt_video: 1280x720@60 paced by the display interrupt ==
display up, pixel clock 74285600 Hz
299 frames; draw worst 3129 us; frame interval worst deviation from 16.667 ms 10 us
599 frames; draw worst 3136 us; frame interval worst deviation from 16.667 ms 10 us
899 frames; draw worst 3136 us; frame interval worst deviation from 16.667 ms 10 us
```

Sixty frames per second, drawn in about 3 ms, and the frame interval never more than 10 microseconds off 16.667 ms while the load task takes 2 ms of every 10.

## 6. Every mode

`apps/base/baremetal/video_modes.rs` cycles through the modes, 12 seconds each, with colour bars and the mode's name. Allow a monitor several seconds to re-sync after each change (with 4 seconds per mode only the last was
seen). Results on a PYNQ-Z2 with one monitor: 800x600, 1024x768, 1280x720, 1280x1024 and 1920x1080 work; **640x480 does not**: the monitor falls back to its own test pattern (tried at 25.175 and 25.2 MHz; the MMCM
settings, the timings and the DMA were fine in each case; the lowest pixel clock, with a 252 Mbit/s TMDS rate per lane, is the suspect, and it is an open item).

## 7. What the first run taught us

The first run showed nothing, and the monitor did not even detect the cable. The stock PYNQ desktop turned out to drive the same pipeline at 1920x1080, so its registers could be read over JTAG while it ran (`xsdb`, target `APU`, which reads
physical addresses) and compared with ours. In the order they were found:

1. **The clock generator's reference.** `axi_dynclk` in this overlay is an older core with eight registers; the reference-frequency register our driver read (offset `0x20`) does not exist. The reference is FCLK0 (100 MHz) and is now a parameter. With it, the words
   written for 148.5 MHz are byte-identical to Linux's.
2. **Where the frame buffers must be.** The VDMA stopped at once with a decode error (`SR` bit 6): the design maps only `0x1000_0000` to `0x1FFF_FFFF` to its HP port, and the buffers were at `0x200000`. They are now in that window, made non-cacheable.
3. **The VDMA's start sequence.** Even with the right addresses the channel halted until the sequence matched Linux's: all four start-address slots written (the frames repeating), no frame-store register write, no frame-count-enable bit.
4. **The video timing controller.** Linux's polarity register is `0x3F` and its vertical sync starts at the end of the front porch (we had a different polarity bit and one line earlier).

Between steps the screen said, in turn: nothing (cable not detected), no signal, black, and then the colour bars: a cheap way to tell how far the pipeline gets.
