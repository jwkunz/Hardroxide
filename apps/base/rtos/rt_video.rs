//! Base overlay, RTOS: a 60 frames-per-second animation paced by the display's own interrupt.
//!
//! The display's frame-done interrupt (the VDMA's, through the interrupt controller) wakes a
//! render task (priority 5) once per frame; it draws into the hidden frame buffer, flips, and
//! measures how long drawing took. A priority-3 task draws nothing but keeps the processor busy
//! with a 2 ms job every 10 ms, and a priority-2 task prints statistics every 5 seconds: the
//! render task keeps its 16.7 ms frame however busy the rest is. Needs `hardroxide-gfx` and
//! `hardroxide-video`. Base overlay application 6.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use design_1::rt::{IrqEvent, Periodic, Shared, WAIT_FOREVER, delay_ms, spawn, start, task_stack};
use design_1::{Design, halt, ps7};
use hardroxide_gfx::{Color, Surface};
use hardroxide_mmio::{PhysMem, time::Instant};
use hardroxide_video::{Display, HD_1280X720_60 as MODE, as_pixels};
use ps7::println;

ps7::entry!(main);

static DESIGN: Shared<Design<PhysMem>> = Shared::new();
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
/// Input of `system_interrupts` fed by the VDMA's read-channel interrupt.
const VDMA_BIT: u32 = 1 << 1;

static FRAMES: AtomicU32 = AtomicU32::new(0);
static DRAW_WORST_US: AtomicU32 = AtomicU32::new(0);
static GAP_WORST_US: AtomicU32 = AtomicU32::new(0);

fn render() {
    let (front, back) = frame_buffers();
    let frames = [FB_BASE as u32, (FB_BASE + FB_STRIDE) as u32];
    let ev = IrqEvent::for_line(&design_1::pac::system_interrupts::IRQ_IRQ).unwrap();
    {
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        for b in [&mut *front, &mut *back] {
            let mut s = Surface::new(as_pixels(b).unwrap(), 1280, 720);
            s.clear(Color::rgb(8, 8, 32));
        }
        let display = Display {
            dynclk: &d.video_hdmi_out_frontend_axi_dynclk,
            ref_clk_hz: 100_000_000, // FCLK0 feeds the pixel clock generator
            vtc: &d.video_hdmi_out_frontend_vtc_out,
            vdma: &d.video_axi_vdma,
            colour: d.video_hdmi_out_color_convert.regs(),
            unpack: d.video_hdmi_out_pixel_unpack.regs(),
        };
        let hz = display.start(&MODE, &frames).unwrap();
        println!("display up, pixel clock {hz} Hz");
        d.system_interrupts.init().unwrap();
        d.system_interrupts.enable(VDMA_BIT).unwrap();
        d.video_axi_vdma.enable_frame_interrupt().unwrap();
    }
    ev.rearm();
    let (mut showing, mut tick) = (0u32, 0u32);
    let mut last = Instant::now();
    loop {
        if !ev.wait(100) {
            continue; // no frame interrupt yet
        }
        let now = Instant::now();
        let gap = now.duration_since(last).as_micros() as u32;
        last = now;
        {
            let d = DESIGN.lock(WAIT_FOREVER).unwrap();
            d.video_axi_vdma.ack_frame().unwrap();
            d.system_interrupts.ack(VDMA_BIT).unwrap();
        }
        ev.rearm();
        if FRAMES.fetch_add(1, Relaxed) > 5 {
            GAP_WORST_US.fetch_max(gap.abs_diff(16_667), Relaxed);
        }
        // Draw the next frame into the hidden buffer.
        let t0 = Instant::now();
        let hidden = if showing == 0 { &mut *back } else { &mut *front };
        let mut s = Surface::new(as_pixels(hidden).unwrap(), 1280, 720);
        tick += 1;
        s.clear(Color::rgb(8, 8, 32));
        for i in 0..8 {
            let phase = tick as i32 * 4 + i * 40;
            let x = 640 + (phase % 1000 - 500) * 5 / 4;
            let y = 360 + ((phase * 3) % 600 - 300);
            s.fill_circle(x, y, 60, Color::hue(((tick as i32 * 2 + i * 32) % 256) as u8));
        }
        s.text(20, 20, "RTOS frame pacing", Color::WHITE, None, 3);
        let draw_us = t0.elapsed().as_micros() as u32;
        DRAW_WORST_US.fetch_max(draw_us, Relaxed);
        showing ^= 1;
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        d.video_axi_vdma.show_frame(showing).unwrap();
    }
}

/// Load: 2 ms of arithmetic every 10 ms, at a priority below the render task.
fn load() {
    let mut every = Periodic::every_ms(10);
    let mut x = 1u32;
    loop {
        every.wait_next();
        let t = Instant::now();
        while t.elapsed().as_micros() < 2000 {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        }
        core::hint::black_box(x);
    }
}

fn report() {
    loop {
        delay_ms(5000);
        println!(
            "{} frames; draw worst {} us; frame interval worst deviation from 16.667 ms {} us",
            FRAMES.load(Relaxed),
            DRAW_WORST_US.load(Relaxed),
            GAP_WORST_US.load(Relaxed)
        );
    }
}

fn main() -> ! {
    let design = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== rt_video: {} paced by the display interrupt ==", MODE.name);
    DESIGN.set(design);
    spawn("render", 5, task_stack!(16384).unwrap(), render).unwrap();
    spawn("load", 3, task_stack!(8192).unwrap(), load).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)
}
