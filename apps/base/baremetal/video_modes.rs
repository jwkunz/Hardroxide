//! Every display mode in turn, 12 seconds each, with the same colour bars and a mode label.
#![no_std]
#![no_main]
use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_gfx::{Color, Surface};
use hardroxide_mmio::time::sleep;
use hardroxide_video::{Display, MODES, as_pixels};
use ps7::println;
ps7::entry!(main);
const FB_BASE: usize = 0x1000_0000;
fn main() -> ! {
    let d = Design::init().unwrap();
    // SAFETY: a private DDR window of 16 MiB (a 1080p frame is 8.3 MB).
    unsafe { ps7::mem::set_non_cacheable(FB_BASE, 16 << 20) };
    let display = Display {
        dynclk: &d.video_hdmi_out_frontend_axi_dynclk, ref_clk_hz: 100_000_000,
        vtc: &d.video_hdmi_out_frontend_vtc_out, vdma: &d.video_axi_vdma,
        colour: d.video_hdmi_out_color_convert.regs(), unpack: d.video_hdmi_out_pixel_unpack.regs(),
    };
    println!("\n== video modes ==");
    for mode in MODES.iter() {
        // SAFETY: as above.
        let fb = unsafe { core::slice::from_raw_parts_mut(FB_BASE as *mut u8, mode.frame_bytes() as usize) };
        let mut s = Surface::new(as_pixels(fb).unwrap(), mode.width as usize, mode.height as usize);
        let (w, h) = (mode.width as i32, mode.height as i32);
        let bars = [Color::WHITE, Color::YELLOW, Color::CYAN, Color::GREEN, Color::MAGENTA, Color::RED, Color::BLUE, Color::BLACK];
        for (i, c) in bars.iter().enumerate() {
            s.fill_rect(i as i32 * w / 8, 0, w / 8 + 1, h * 3 / 4, *c);
        }
        for x in 0..w { let g = (x * 255 / w) as u8; s.vline(x, h * 3 / 4, h / 4, Color::rgb(g, g, g)); }
        s.rect(0, 0, w, h, Color::WHITE);
        s.text(40, h / 3, mode.name, Color::BLACK, Some(Color::WHITE), 5);
        let r = display.stop().ok().and(display.start(mode, &[FB_BASE as u32]).ok());
        println!("{}: pixel clock {:?}, dma errors {:?}", mode.name, r, display.check().is_err());
        sleep(Duration::from_secs(12));
    }
    println!("done");
    halt()
}
