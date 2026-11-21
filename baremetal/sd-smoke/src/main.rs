//! End-to-end test of the SD host driver and the FAT32 file system: initialise the card, format it,
//! write files and directories, read them back and check them, and report the speeds. Under QEMU the
//! "card" is a disk image (`scripts/test-qemu.sh` then runs `fsck.fat` on it); the same program runs
//! on a board over JTAG with a card in the slot **and erases the card**, so it is built only with the
//! `qemu` feature.

#![no_std]
#![no_main]

use hardroxide_fat::{FormatOptions, OpenMode, Timestamp, Volume};
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};
use hardroxide_ps7::sd::SdHost;
use hardroxide_ps7::{clocks, console, map, println, time};

hardroxide_ps7::entry!(main);

const PS_CLK_HZ: u64 = if cfg!(feature = "qemu") { 33_333_333 } else { 50_000_000 };

fn platform_init() {
    // SAFETY: this program owns the SLCR for the duration of this setup.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let w = |o: usize, v: u32| slcr.write32(o, v).unwrap();
    let r = |o: usize| slcr.read32(o).unwrap();
    w(0x008, 0xDF0D);
    if cfg!(feature = "qemu") {
        let m = |a: usize, mask: u32, v: u32| w(a, (r(a) & !mask) | (v & mask));
        m(0x108, 0x0007_F000, 0x0002_0000);
        m(0x108, 0x10, 0x10);
        m(0x108, 0x1, 0x1);
        m(0x108, 0x1, 0x0);
        m(0x108, 0x10, 0x0);
    }
    w(0x12C, r(0x12C) | (1 << 20));
    let io_fdiv = (r(0x108) >> 12) & 0x7F;
    w(0x154, ((io_fdiv / 2).max(1) << 8) | 0x3);
    w(0x228, 0);
    w(0x738, 0x16E1);
    w(0x73C, 0x16E0);
    w(0x004, 0x767B);
}

/// A deterministic byte pattern.
fn byte(seed: u32, i: usize) -> u8 {
    (i as u32).wrapping_mul(2_654_435_761).wrapping_add(seed).rotate_left(7) as u8
}

fn fail(msg: &str) -> ! {
    println!("SD SMOKE FAILED: {msg}");
    loop { hardroxide_ps7::cp15::wfe() }
}

fn main() -> ! {
    platform_init();
    // SAFETY: early in start-up, running from DDR at its physical address.
    unsafe { hardroxide_ps7::mem::enable(512 << 20) };
    // SAFETY: the SLCR is only read here; the console UART is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let uart_ref = clocks::uart_ref_hz(&slcr, PS_CLK_HZ).unwrap();
    // SAFETY: as above.
    unsafe { console::init(map::UART0_BASE, uart_ref as u32, 115_200) };
    let periph = clocks::periph_hz(&slcr, PS_CLK_HZ).unwrap();
    // SAFETY: nothing else touches the global timer.
    unsafe { time::init(periph) };
    println!("\n== SD card and FAT32 smoke test ==");

    let base = if cfg!(feature = "qemu") { 50_000_000 } else { clocks::sdio_ref_hz(&slcr, PS_CLK_HZ).unwrap() as u32 };
    let mut sd = SdHost::new(be.map(map::SDIO0_BASE as u64, map::SDIO_SIZE).unwrap(), base);
    let info = match sd.init() {
        Ok(i) => i,
        Err(e) => fail(&alloc_free(format_args!("card initialisation: {e}"))),
    };
    println!(
        "card: {} blocks ({} MiB), {}, rca {:#x}, bus {} Hz",
        info.blocks,
        info.blocks * 512 / (1 << 20),
        if info.high_capacity { "SDHC/SDXC" } else { "standard capacity" },
        info.rca,
        info.clock_hz
    );

    // Raw blocks: write a pattern at the end of the card, read it back.
    let mut block = [0u8; 512 * 4];
    for (i, b) in block.iter_mut().enumerate() {
        *b = byte(1, i);
    }
    let last = info.blocks - 8;
    sd.write(last, &block).unwrap_or_else(|e| fail(&alloc_free(format_args!("raw write: {e}"))));
    let mut back = [0u8; 512 * 4];
    sd.read(last, &mut back).unwrap_or_else(|e| fail(&alloc_free(format_args!("raw read: {e}"))));
    if back != block {
        fail("raw block read-back differs");
    }
    println!("raw blocks: 4 blocks written and read back at the end of the card");

    // The file system.
    Volume::format(&mut sd, &FormatOptions::default()).unwrap_or_else(|e| fail(&alloc_free(format_args!("format: {e}"))));
    let mut vol = Volume::mount(sd).unwrap_or_else(|e| fail(&alloc_free(format_args!("mount: {e}"))));
    vol.set_time(Timestamp { year: 2026, month: 11, day: 21, hour: 15, minute: 0, second: 0 });
    println!("formatted and mounted: {} MiB free, {} byte clusters", vol.free_bytes().unwrap() >> 20, vol.cluster_bytes());

    vol.mkdir("/logs").unwrap();
    {
        let mut f = vol.open("/logs/report.txt", OpenMode::Write).unwrap();
        f.write(&mut vol, b"hardroxide SD smoke test\n").unwrap();
        f.close(&mut vol).unwrap();
    }
    {
        let mut f = vol.open("/A long file name for the smoke test.txt", OpenMode::Write).unwrap();
        f.write(&mut vol, b"long names work\n").unwrap();
        f.close(&mut vol).unwrap();
    }

    // A megabyte, written in 4 KiB pieces and read back.
    const MIB: usize = 1 << 20;
    let mut chunk = [0u8; 4096];
    let t0 = time::now_ns();
    {
        let mut f = vol.open("/big.bin", OpenMode::Write).unwrap();
        for n in 0..MIB / 4096 {
            for (i, b) in chunk.iter_mut().enumerate() {
                *b = byte(7, n * 4096 + i);
            }
            if f.write(&mut vol, &chunk).unwrap() != 4096 {
                fail("short write");
            }
        }
        f.close(&mut vol).unwrap();
    }
    let t1 = time::now_ns();
    {
        let mut f = vol.open("/big.bin", OpenMode::Read).unwrap();
        if f.len() as usize != MIB {
            fail("big.bin has the wrong size");
        }
        for n in 0..MIB / 4096 {
            if f.read(&mut vol, &mut chunk).unwrap() != 4096 {
                fail("short read");
            }
            for (i, b) in chunk.iter().enumerate() {
                if *b != byte(7, n * 4096 + i) {
                    fail("big.bin read-back differs");
                }
            }
        }
    }
    let t2 = time::now_ns();
    println!("1 MiB written in {} ms, read in {} ms (4 KiB pieces, programmed I/O)", (t1 - t0) / 1_000_000, (t2 - t1) / 1_000_000);

    let mut names = 0;
    vol.read_dir("/", |e| {
        println!("  {} {:>8} {}", if e.is_dir { "d" } else { "-" }, e.size, e.name);
        names += 1;
    })
    .unwrap();
    if names != 3 {
        fail("the root directory should hold 3 entries");
    }
    vol.sync().unwrap();
    println!("SD SMOKE OK");
    loop { hardroxide_ps7::cp15::wfe() }
}

/// Format a message into a small stack buffer (no allocator here).
fn alloc_free(args: core::fmt::Arguments) -> heapless_str::Str {
    heapless_str::Str::new(args)
}

mod heapless_str {
    use core::fmt::Write;
    pub struct Str {
        buf: [u8; 160],
        len: usize,
    }
    impl Str {
        pub fn new(args: core::fmt::Arguments) -> Self {
            let mut s = Str { buf: [0; 160], len: 0 };
            let _ = s.write_fmt(args);
            s
        }
    }
    impl Write for Str {
        fn write_str(&mut self, t: &str) -> core::fmt::Result {
            let n = t.len().min(self.buf.len() - self.len);
            self.buf[self.len..self.len + n].copy_from_slice(&t.as_bytes()[..n]);
            self.len += n;
            Ok(())
        }
    }
    impl core::ops::Deref for Str {
        type Target = str;
        fn deref(&self) -> &str {
            core::str::from_utf8(&self.buf[..self.len]).unwrap_or("?")
        }
    }
}
