//! Base overlay, bare metal: save a picture from the screen to the SD card as a BMP, and show one from the card.
//!
//! A test picture (colour bars, a gradient, text) is drawn and shown on the HDMI output at 1280x720. The program
//! saves it to `/screen.bmp` (a standard 24-bit BMP), blanks the screen, loads the file
//! back from the card row by row onto the screen, and compares every pixel with a fresh copy of the picture.
//! The card must carry a FAT32 file system (`sd_erase_and_test` makes one). **It writes only to the card named when
//! building** (`HRX_WRITE_SERIAL=0x...`, the serial number `sd_info` prints) and leaves any other untouched.
//! Needs `hardroxide-gfx`, `hardroxide-video` and `hardroxide-fat`. Base overlay application 16.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_fat::{OpenMode, Volume};
use hardroxide_gfx::bmp::{BmpInfo, encode_row, header_24bit};
use hardroxide_gfx::{Color, Surface};
use hardroxide_mmio::time::sleep;
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_video::{Display, HD_1280X720_60 as MODE, as_pixels};
use ps7::println;
use ps7::sd::SdHost;

ps7::entry!(main);

const W: usize = 1280;
const H: usize = 720;
const FRAME_BYTES: usize = W * H * 4;
/// Frame buffers in the DDR window the video DMA reaches, made non-cacheable (see `video_demo`).
const FB_BASE: usize = 0x1000_0000;
const FB_STRIDE: usize = 4 << 20;

fn fail(what: &str, e: &dyn core::fmt::Debug) -> ! {
    println!("SD BMP FAILED: {what}: {e:?}");
    halt()
}

const BARS: [Color; 8] = [Color::WHITE, Color::YELLOW, Color::CYAN, Color::GREEN, Color::MAGENTA, Color::RED, Color::BLUE, Color::BLACK];

fn picture(s: &mut Surface) {
    let (w, h) = (s.width() as i32, s.height() as i32);
    s.clear(Color::rgb(16, 16, 24));
    let bw = w / 8;
    for (i, c) in BARS.iter().enumerate() {
        s.fill_rect(i as i32 * bw, 0, bw, h / 2, *c);
    }
    for x in 0..w {
        s.vline(x, h / 2, h / 8, Color::hue((x * 255 / w) as u8));
        let g = (x * 255 / w) as u8;
        s.vline(x, h / 2 + h / 8, h / 8, Color::rgb(g, g, g));
    }
    s.rect(1, 1, w - 2, h - 2, Color::WHITE);
    s.text(24, h * 3 / 4 + 12, "HARDROXIDE", Color::WHITE, None, 6);
    s.text(24, h * 3 / 4 + 80, "this picture came from the SD card (screen.bmp)", Color::YELLOW, None, 2);
}

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    // SAFETY: the SLCR is read here; the SDIO 0 window is used by nothing else; the DDR window above is used by
    // nothing else (the program and its stacks are low in DDR) and the MMU is already on.
    let be = unsafe { PhysMem::new() };
    let (front, back) = unsafe {
        ps7::mem::set_non_cacheable(FB_BASE, 2 * FB_STRIDE);
        (
            core::slice::from_raw_parts_mut(FB_BASE as *mut u8, FRAME_BYTES),
            core::slice::from_raw_parts_mut((FB_BASE + FB_STRIDE) as *mut u8, FRAME_BYTES),
        )
    };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let base = ps7::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32;
    let mut sd = SdHost::new(be.map(ps7::map::SDIO0_BASE as u64, ps7::map::SDIO_SIZE).unwrap(), base);
    println!("\n== sd_bmp ==");
    let card = sd.init().or_else(|_| sd.init_ignoring_card_detect()).unwrap_or_else(|e| fail("card initialisation", &e));
    if !card.serial_matches(option_env!("HRX_WRITE_SERIAL")) {
        println!("REFUSING: this program writes files to the card. To allow that for the card in the slot, build it with HRX_WRITE_SERIAL={:#010x}", card.serial);
        println!("SD BMP FAILED: not allowed to write");
        halt()
    }
    let mut vol = Volume::mount(sd).unwrap_or_else(|e| fail("mount (format the card with sd_erase_and_test first)", &e));

    let display = Display {
        dynclk: &d.video_hdmi_out_frontend_axi_dynclk,
        ref_clk_hz: 100_000_000,
        vtc: &d.video_hdmi_out_frontend_vtc_out,
        vdma: &d.video_axi_vdma,
        colour: d.video_hdmi_out_color_convert.regs(),
        unpack: d.video_hdmi_out_pixel_unpack.regs(),
    };
    picture(&mut Surface::new(as_pixels(front).unwrap(), W, H));
    if let Err(e) = display.start(&MODE, &[FB_BASE as u32, (FB_BASE + FB_STRIDE) as u32]) {
        println!("display start failed: {e}");
        halt()
    }
    println!("showing the picture");
    sleep(Duration::from_secs(2));

    // Save: a header, then the rows from the bottom of the picture to the top.
    let t0 = ps7::time::now_ns();
    {
        let mut f = vol.open("/screen.bmp", OpenMode::Write).unwrap_or_else(|e| fail("create /screen.bmp", &e));
        f.write(&mut vol, &header_24bit(W as u32, H as u32)).unwrap_or_else(|e| fail("write header", &e));
        let pixels = as_pixels(front).unwrap();
        let mut row = [0u8; (W * 3).next_multiple_of(4)];
        for y in (0..H).rev() {
            encode_row(&pixels[y * W..(y + 1) * W], &mut row);
            f.write(&mut vol, &row).unwrap_or_else(|e| fail("write row", &e));
        }
        f.close(&mut vol).unwrap_or_else(|e| fail("close", &e));
    }
    vol.sync().unwrap_or_else(|e| fail("sync", &e));
    let ms = (ps7::time::now_ns() - t0) / 1_000_000;
    println!("saved /screen.bmp: {} bytes in {ms} ms", 54 + (W * 3).next_multiple_of(4) * H);

    // Blank the screen, then load the file back onto it, row by row.
    Surface::new(as_pixels(front).unwrap(), W, H).clear(Color::BLACK);
    sleep(Duration::from_secs(1));
    let t0 = ps7::time::now_ns();
    let mut f = vol.open("/screen.bmp", OpenMode::Read).unwrap_or_else(|e| fail("open /screen.bmp", &e));
    let mut header = [0u8; 54];
    if f.read(&mut vol, &mut header).unwrap_or_else(|e| fail("read header", &e)) != 54 {
        println!("SD BMP FAILED: short header");
        halt()
    }
    let info = BmpInfo::parse(&header).unwrap_or_else(|e| fail("parse header", &e));
    if (info.width as usize, info.height as usize) != (W, H) {
        println!("SD BMP FAILED: the file is {}x{}", info.width, info.height);
        halt()
    }
    let pixels = as_pixels(front).unwrap();
    let mut row = [0u8; (W * 3).next_multiple_of(4)];
    for y in 0..H {
        f.seek(&mut vol, info.row_offset(y as u32) as u32).unwrap_or_else(|e| fail("seek", &e));
        let n = f.read(&mut vol, &mut row[..info.row_bytes as usize]).unwrap_or_else(|e| fail("read row", &e));
        if n != info.row_bytes as usize {
            println!("SD BMP FAILED: short row {y}");
            halt()
        }
        info.convert_row(&row, &mut pixels[y * W..(y + 1) * W]);
    }
    let ms = (ps7::time::now_ns() - t0) / 1_000_000;
    println!("loaded it back in {ms} ms");

    // Compare with a fresh copy of the picture, in the other buffer (not shown).
    picture(&mut Surface::new(as_pixels(back).unwrap(), W, H));
    let (a, b) = (as_pixels(front).unwrap(), as_pixels(back).unwrap());
    let different = a.iter().zip(b.iter()).filter(|(x, y): &(&u32, &u32)| (*x & 0x00FF_FFFF) != (*y & 0x00FF_FFFF)).count();
    println!("{different} pixels differ from the original");
    println!("{}", if different == 0 { "SD BMP OK" } else { "SD BMP FAILED: the picture came back different" });
    halt()
}
