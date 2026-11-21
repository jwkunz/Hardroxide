//! Base overlay, bare metal: HDMI output at 1280x720 with the graphics library.
//!
//! Two frame buffers in the window of DDR the video DMA can reach, made non-cacheable. A test pattern (colour bars, a
//! gradient, a grid and some text) is drawn into one, shown, and then a bouncing ball is animated
//! with double buffering for ten seconds: draw into the hidden buffer, wait for the end of the
//! frame, switch the display to it. Needs `hardroxide-gfx` and `hardroxide-video` (`cargo add --path`).
//! Base overlay application 5.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_gfx::{Color, Surface};
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_video::{Display, HD_1280X720_60 as MODE, as_pixels};
use ps7::println;

ps7::entry!(main);

const FRAME_BYTES: usize = 1280 * 720 * 4;
/// The frame buffers live in the window of DDR that this overlay's video DMA can reach
/// (0x1000_0000 to 0x1FFF_FFFF), made non-cacheable so the processor's drawing and the DMA's reads agree.
const FB_BASE: usize = 0x1000_0000;
/// 4 MiB per frame: a 1280x720 frame is 3.5 MiB, rounded up to whole 1 MiB sections.
const FB_STRIDE: usize = 4 << 20;

fn frame_buffers() -> (&'static mut [u8], &'static mut [u8]) {
    // SAFETY: nothing else in the program uses this DDR window (the program, its data and stacks are low in
    // DDR), and `Design::init` has already turned the MMU on.
    unsafe {
        ps7::mem::set_non_cacheable(FB_BASE, 2 * FB_STRIDE);
        (
            core::slice::from_raw_parts_mut(FB_BASE as *mut u8, FRAME_BYTES),
            core::slice::from_raw_parts_mut((FB_BASE + FB_STRIDE) as *mut u8, FRAME_BYTES),
        )
    }
}

const BARS: [Color; 8] = [Color::WHITE, Color::YELLOW, Color::CYAN, Color::GREEN, Color::MAGENTA, Color::RED, Color::BLUE, Color::BLACK];

/// The colour bars of the top half, painted into the rectangle `(x, y, w, h)` only.
fn bars(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    let bw = s.width() as i32 / 8;
    let (x0, x1) = (x.max(0), (x + w).min(s.width() as i32));
    for bx in (x0 / bw)..=((x1 - 1) / bw).min(7) {
        let lo = (bx * bw).max(x0);
        let hi = ((bx + 1) * bw).min(x1);
        s.fill_rect(lo, y, hi - lo, h, BARS[bx as usize]);
    }
}

fn test_pattern(s: &mut Surface) {
    let (w, h) = (s.width() as i32, s.height() as i32);
    s.clear(Color::rgb(16, 16, 24));
    bars(s, 0, 0, w, h / 2);
    // A hue gradient and a gray ramp.
    for x in 0..w {
        s.vline(x, h / 2, h / 8, Color::hue((x * 255 / w) as u8));
        let g = (x * 255 / w) as u8;
        s.vline(x, h / 2 + h / 8, h / 8, Color::rgb(g, g, g));
    }
    // A grid, to check the geometry: lines every 80 pixels, the border one pixel in.
    for x in (0..w).step_by(80) {
        s.vline(x, h * 3 / 4, h / 4, Color::GRAY);
    }
    for y in (h * 3 / 4..h).step_by(80) {
        s.hline(0, y, w, Color::GRAY);
    }
    s.rect(1, 1, w - 2, h - 2, Color::WHITE);
    s.text(24, h * 3 / 4 + 12, "HARDROXIDE", Color::WHITE, None, 6);
    s.text(24, h * 3 / 4 + 80, "1280x720 over HDMI: a bare-metal Rust program, no operating system", Color::YELLOW, None, 2);
}

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let (front, back) = frame_buffers();
    let frames = [FB_BASE as u32, (FB_BASE + FB_STRIDE) as u32];

    let display = Display {
        dynclk: &d.video_hdmi_out_frontend_axi_dynclk,
        ref_clk_hz: 100_000_000, // FCLK0 feeds the pixel clock generator
        vtc: &d.video_hdmi_out_frontend_vtc_out,
        vdma: &d.video_axi_vdma,
        colour: d.video_hdmi_out_color_convert.regs(),
        unpack: d.video_hdmi_out_pixel_unpack.regs(),
    };

    {
        let mut s = Surface::new(as_pixels(front).unwrap(), 1280, 720);
        test_pattern(&mut s);
    }
    println!("\n== video_demo: {} ==", MODE.name);
    match display.start(&MODE, &frames) {
        Ok(hz) => println!("pixel clock {hz} Hz (asked {})", MODE.pixel_clock_hz),
        Err(e) => {
            println!("display start failed: {e}");
            halt()
        }
    }
    println!("showing the test pattern for 5 s");
    sleep(Duration::from_secs(5));

    // Double-buffered animation: a ball bouncing over the colour bars. Each buffer remembers where the ball
    // was drawn in it, so only that spot and the new one are repainted (a full redraw of non-cacheable memory is slow).
    {
        let mut s = Surface::new(as_pixels(back).unwrap(), 1280, 720);
        test_pattern(&mut s);
    }
    println!("animating for 10 s");
    const R: i32 = 40;
    let (mut x, mut y, mut dx, mut dy) = (200i32, 150i32, 7i32, 5i32);
    let mut drawn = [(200i32, 150i32); 2];
    let t0 = Instant::now();
    let mut frames_shown = 0u32;
    let mut showing = 0u32;
    while t0.elapsed() < Duration::from_secs(10) {
        {
            let hidden = if showing == 0 { &mut *back } else { &mut *front };
            let idx = (showing ^ 1) as usize;
            let mut s = Surface::new(as_pixels(hidden).unwrap(), 1280, 720);
            let (ox, oy) = drawn[idx];
            bars(&mut s, ox - R - 2, oy - R - 2, 2 * R + 5, 2 * R + 5);
            s.fill_circle(x, y, R, Color::hue((frames_shown * 3 % 256) as u8));
            s.circle(x, y, R + 1, Color::WHITE);
            drawn[idx] = (x, y);
        }
        showing ^= 1;
        display.show(showing).unwrap();
        // The switch happens at the next frame boundary; once that frame has finished the old buffer is free.
        let _ = display.wait_frame(Duration::from_millis(100));
        frames_shown += 1;
        x += dx;
        y += dy;
        if x < R + 2 || x > 1280 - R - 2 {
            dx = -dx;
        }
        if y < R + 2 || y > 720 / 2 - R - 2 {
            dy = -dy;
        }
    }
    let ms = t0.elapsed().as_millis() as u32;
    println!(
        "{frames_shown} animated frames in {ms} ms = {} frames per second; display errors: {:?}",
        frames_shown * 1000 / ms.max(1),
        display.check().is_err()
    );
    halt()
}
