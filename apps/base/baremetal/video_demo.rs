//! Base overlay, bare metal: HDMI output at 1280x720 with the graphics library.
//!
//! Two frame buffers in the non-cacheable `.dma` section. A test pattern (colour bars, a
//! gradient, a grid and some text) is drawn into one, shown, and then a bouncing ball is animated
//! with double buffering for ten seconds: draw into the hidden buffer, wait for the end of the
//! frame, switch the display to it. Needs `hardroxide-gfx` and `hardroxide-video` (`cargo add --path`).
//! Base overlay application 5.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_dma::{DmaBuffer, static_dma_buffer};
use hardroxide_gfx::{Color, Surface};
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_video::{Display, HD_1280X720_60 as MODE, as_pixels};
use ps7::println;

ps7::entry!(main);

const FRAME_BYTES: usize = 1280 * 720 * 4;

fn test_pattern(s: &mut Surface) {
    let (w, h) = (s.width() as i32, s.height() as i32);
    s.clear(Color::rgb(16, 16, 24));
    // Colour bars across the top half.
    let bars = [Color::WHITE, Color::YELLOW, Color::CYAN, Color::GREEN, Color::MAGENTA, Color::RED, Color::BLUE, Color::BLACK];
    let bw = w / 8;
    for (i, c) in bars.iter().enumerate() {
        s.fill_rect(i as i32 * bw, 0, bw, h / 2, *c);
    }
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
    let mut front = static_dma_buffer!(FRAME_BYTES).unwrap();
    let mut back = static_dma_buffer!(FRAME_BYTES).unwrap();
    let frames = [front.phys_addr() as u32, back.phys_addr() as u32];

    let display = Display {
        dynclk: &d.video_hdmi_out_frontend_axi_dynclk,
        ref_clk_hz: 100_000_000, // FCLK0 feeds the pixel clock generator
        vtc: &d.video_hdmi_out_frontend_vtc_out,
        vdma: &d.video_axi_vdma,
        colour: d.video_hdmi_out_color_convert.regs(),
        unpack: d.video_hdmi_out_pixel_unpack.regs(),
    };

    {
        let mut s = Surface::new(as_pixels(front.as_mut_slice()).unwrap(), 1280, 720);
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

    // Double-buffered animation: a ball bouncing over the pattern.
    println!("animating for 10 s");
    let (mut x, mut y, mut dx, mut dy) = (200i32, 150i32, 7i32, 5i32);
    let t0 = Instant::now();
    let mut frames_shown = 0u32;
    let mut showing = 0u32;
    while t0.elapsed() < Duration::from_secs(10) {
        {
            let hidden = if showing == 0 { &mut back } else { &mut front };
            let mut s = Surface::new(as_pixels(hidden.as_mut_slice()).unwrap(), 1280, 720);
            test_pattern(&mut s);
            s.fill_circle(x, y, 40, Color::hue((frames_shown * 3 % 256) as u8));
            s.circle(x, y, 41, Color::WHITE);
        }
        showing ^= 1;
        display.show(showing).unwrap();
        // Wait until the display has switched (the next frame boundary) before drawing over the old one.
        let _ = display.wait_frame(Duration::from_millis(100));
        let _ = display.wait_frame(Duration::from_millis(100));
        frames_shown += 1;
        x += dx;
        y += dy;
        if x < 41 || x > 1280 - 41 {
            dx = -dx;
        }
        if y < 41 || y > 720 / 2 - 41 {
            dy = -dy;
        }
    }
    let ms = t0.elapsed().as_millis() as u32;
    println!("{frames_shown} animated frames in {ms} ms = {} frames per second; display errors: {:?}", frames_shown * 1000 / ms.max(1), display.check().is_err());
    halt()
}
