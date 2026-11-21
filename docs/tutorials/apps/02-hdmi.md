# Applications 2: Pixels on the HDMI output

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 1](01-base-lights.md), an HDMI monitor on the board's **HDMI OUT**

> **Status: written, not yet run on a board.** Everything below compiles and the register-level code is unit-tested, but the first
> run on the board (and on a monitor) is still to be done; the "what you should see" parts are expectations, not captured output.
> The project's rule is that nothing is called verified until it has been, so this page will be updated with real output after that
> run ([Verification gaps](../../08-verification-gaps.md)).

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
bouncing ball for ten with double buffering. On the console it prints the pixel clock the MMCM produced (expected: within 0.1 % of
74.25 MHz) and the number of animated frames. On the monitor: eight colour bars, a hue gradient and a gray ramp, a grid, and the text
HARDROXIDE.

## 3. The two frame buffers

```rust,ignore
let mut front = static_dma_buffer!(FRAME_BYTES).unwrap();     // 1280 x 720 x 4 bytes, non-cacheable
let mut back  = static_dma_buffer!(FRAME_BYTES).unwrap();
let frames = [front.phys_addr() as u32, back.phys_addr() as u32];
...
let mut s = Surface::new(as_pixels(front.as_mut_slice()).unwrap(), 1280, 720);
s.fill_circle(x, y, 40, Color::hue(h));
```

Both buffers live in the `.dma` section, which the MMU maps non-cacheable, so the CPU's drawing and the VDMA's reads see the same bytes with no
cache maintenance. The cost, measured for DMA in [Bare metal 3](../baremetal/03-dma.md), is that drawing goes straight to DDR; at 720p a
full-screen redraw is 3.5 MiB of writes. A cached buffer with `clean_range` before each flip is the faster variant and is future work.

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
to keep the frame interval steady however busy the rest is. Build it as in section 2 with the RTOS crate (`--platform rtos`).

## 6. What to check, and what to tell us

When you run it for the first time: does the monitor sync (any picture at all)? Are the colours right (red is red, not blue: the colour
matrix is the identity for `0x00RRGGBB`, and `colour::SWAP_RB` swaps the channels if not)? Is the image stable at 1280x720, and what do the 640x480 and 1920x1080
modes do? What frame rate does the animation reach? The answers go into [Verification gaps](../../08-verification-gaps.md) and, with the real console output,
into this page.
