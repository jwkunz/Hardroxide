//! Base overlay, bare metal: a data logger that appends to a file on the SD card.
//!
//! Every run appends ten timestamped lines to `/LOG.TXT` (creating it the first time), then reads the whole file
//! back and checks that the earlier runs' lines are still there and the new ones follow them. Run it several times:
//! the run number goes up and the file grows, and the card can be read on a PC afterwards (a plain text file).
//! The card must carry a FAT32 file system (`sd_erase_and_test` makes one). **It writes only to the card named when
//! building** (`HRX_WRITE_SERIAL=0x...`, the serial number `sd_info` prints) and leaves any other untouched.
//! Needs `hardroxide-fat`. Base overlay application 17.

#![no_std]
#![no_main]

use design_1::{Design, halt, ps7};
use hardroxide_fat::{OpenMode, Volume};
use hardroxide_mmio::{MmioBackend, PhysMem};
use ps7::println;
use ps7::sd::SdHost;

ps7::entry!(main);

const LINES: usize = 10;

fn fail(what: &str, e: &dyn core::fmt::Debug) -> ! {
    println!("SD LOGGER FAILED: {what}: {e:?}");
    halt()
}

/// Format into a byte buffer; returns the length.
fn fmt(buf: &mut [u8], args: core::fmt::Arguments) -> usize {
    use core::fmt::Write;
    struct W<'a>(&'a mut [u8], usize);
    impl Write for W<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let n = s.len().min(self.0.len() - self.1);
            self.0[self.1..self.1 + n].copy_from_slice(&s.as_bytes()[..n]);
            self.1 += n;
            Ok(())
        }
    }
    let mut w = W(buf, 0);
    let _ = w.write_fmt(args);
    w.1
}

/// The number of lines (newline characters) and the size in bytes of the file.
fn scan<D: hardroxide_fat::BlockDevice>(vol: &mut Volume<D>, path: &str) -> (usize, u32) {
    let mut f = vol.open(path, OpenMode::Read).unwrap_or_else(|e| fail("open for reading", &e));
    let size = f.len();
    let mut lines = 0usize;
    let mut buf = [0u8; 512];
    loop {
        let n = f.read(vol, &mut buf).unwrap_or_else(|e| fail("read", &e));
        if n == 0 {
            break;
        }
        lines += buf[..n].iter().filter(|&&b| b == b'\n').count();
    }
    (lines, size)
}

fn main() -> ! {
    let _d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    // SAFETY: the SLCR is read here; the SDIO 0 window is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(ps7::map::SLCR_BASE as u64, ps7::map::SLCR_SIZE).unwrap();
    let base = ps7::clocks::sdio_ref_hz(&slcr, 50_000_000).unwrap() as u32;
    let mut sd = SdHost::new(be.map(ps7::map::SDIO0_BASE as u64, ps7::map::SDIO_SIZE).unwrap(), base);
    println!("\n== sd_logger ==");
    let card = sd.init().or_else(|_| sd.init_ignoring_card_detect()).unwrap_or_else(|e| fail("card initialisation", &e));
    if !card.serial_matches(option_env!("HRX_WRITE_SERIAL")) {
        println!("REFUSING: this program writes files to the card. To allow that for the card in the slot, build it with HRX_WRITE_SERIAL={:#010x}", card.serial);
        println!("SD LOGGER FAILED: not allowed to write");
        halt()
    }
    let mut vol = Volume::mount(sd).unwrap_or_else(|e| fail("mount (format the card with sd_erase_and_test first)", &e));

    let before = if vol.stat("/LOG.TXT").is_ok() { scan(&mut vol, "/LOG.TXT").0 } else { 0 };
    let run = before / LINES + 1;
    println!("/LOG.TXT has {before} lines: this is run {run}");

    let mut f = vol.open("/LOG.TXT", OpenMode::Append).unwrap_or_else(|e| fail("open /LOG.TXT", &e));
    for i in 0..LINES {
        let mut line = [0u8; 96];
        let n = fmt(&mut line, format_args!("run {run} line {i} at {} ms\n", ps7::time::now_ns() / 1_000_000));
        f.write(&mut vol, &line[..n]).unwrap_or_else(|e| fail("append", &e));
    }
    f.close(&mut vol).unwrap_or_else(|e| fail("close", &e));
    vol.sync().unwrap_or_else(|e| fail("sync", &e));

    let (after, size) = scan(&mut vol, "/LOG.TXT");
    println!("/LOG.TXT now has {after} lines, {size} bytes");
    if after != before + LINES {
        println!("SD LOGGER FAILED: expected {} lines", before + LINES);
        halt()
    }
    // The last line must be the one just written.
    let mut f = vol.open("/LOG.TXT", OpenMode::Read).unwrap();
    let mut tail = [0u8; 96];
    let from = size.saturating_sub(tail.len() as u32);
    f.seek(&mut vol, from).unwrap();
    let n = f.read(&mut vol, &mut tail).unwrap();
    let text = core::str::from_utf8(&tail[..n]).unwrap_or("?");
    let last = text.trim_end().rsplit('\n').next().unwrap_or("");
    println!("last line: {last}");
    let mut expect = [0u8; 32];
    let m = fmt(&mut expect, format_args!("run {run} line {} at ", LINES - 1));
    if !last.starts_with(core::str::from_utf8(&expect[..m]).unwrap()) {
        println!("SD LOGGER FAILED: the last line is not the one just written");
        halt()
    }
    println!("SD LOGGER OK");
    halt()
}
