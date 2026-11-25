//! The RFSoC 4x2's own hardware on the real-time kernel: the same chips as `rfsoc-board`, each looked after by a task
//! with its own deadline, so that slow work never delays urgent work.
//!
//! | task | priority | what it does |
//! |---|---|---|
//! | `heartbeat` | 6 | blinks the processor's first LED, exactly twice a second |
//! | `button` | 5 | looks at the push button every 10 ms; a press steps the display's page |
//! | `monitor` | 4 | reads the nine power monitors and the temperature every 500 ms |
//! | `check` | 3 | after five seconds, prints how everyone did and the verdict, then ends |
//! | `display` | 2 | draws the current page when asked, and twice a second |
//! | `clocks` | 1 | programs the radio clock chips, once: a long job that disturbs nobody |
//!
//! Each chip belongs to one task. What is shared is the latest readings, behind a mutex held only to copy them in or
//! out (never while talking to a slow chip), and the console. Nothing permanent is written.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use core::time::Duration;
use hardroxide_board::at24mac::{self, MacAddress};
use hardroxide_board::ina220::{self, Reading};
use hardroxide_board::rfsoc4x2 as board;
use hardroxide_board::us2066::Oled;
use hardroxide_board::{lmk04828, lmx2594, tics};
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use hardroxide_rt::{
    Mutex, Periodic, Semaphore, Shared, WAIT_FOREVER, cpu_total_ticks, delay_ms, spawn, start,
    task_info, task_slots, task_stack,
};
use platform::ps::{self, println};
use ps::gpio::Gpio;
use ps::i2c::I2c;
use ps::spi::{Mode, Spi};
use ps::sysmon::{self, Sensor};

#[allow(dead_code)]
#[path = "../../common/pages.rs"]
mod pages;
#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;
use pages::{Line, PAGES};

ps::entry!(main);

const PS_REF_HZ: u64 = 33_333_333;
const LMK_FILE: &str = include_str!("../../../fixtures/rfsoc4x2/clocks/LMK04828_245.76.txt");
const LMX_FILE: &str = include_str!("../../../fixtures/rfsoc4x2/clocks/LMX2594_491.52.txt");

/// The latest of everything `monitor` reads.
#[derive(Clone, Copy)]
struct Readings {
    rails: [Option<Reading>; board::RAILS.len()],
    temperature: Option<i32>,
}

static READINGS: Shared<Readings> = Shared::new();
static MAC: Shared<MacAddress> = Shared::new();
static CONSOLE: Mutex = Mutex::new();
/// Asks `display` to draw now.
static REDRAW: Semaphore = Semaphore::new(0, 1);

static PAGE: AtomicUsize = AtomicUsize::new(0);
static BEATS: AtomicU32 = AtomicU32::new(0);
static HEARTBEAT_WORST_NS: AtomicU32 = AtomicU32::new(0);
static BUTTON_WORST_NS: AtomicU32 = AtomicU32::new(0);
static UPDATES: AtomicU32 = AtomicU32::new(0);
static BAD_UPDATES: AtomicU32 = AtomicU32::new(0);
static MONITOR_LONGEST_US: AtomicU32 = AtomicU32::new(0);
static DRAWS: AtomicU32 = AtomicU32::new(0);
static DRAW_LONGEST_US: AtomicU32 = AtomicU32::new(0);
static DISPLAY_FOLLOWS: AtomicBool = AtomicBool::new(true);
static CLOCKS_LOCKED: AtomicBool = AtomicBool::new(false);
static CLOCKS_MS: AtomicU32 = AtomicU32::new(0);

/// Print a line from a task, whole: tasks share the console.
macro_rules! say {
    ($($arg:tt)*) => {{
        let _console = CONSOLE.lock(WAIT_FOREVER);
        println!($($arg)*);
    }};
}

fn map(base: usize, size: usize) -> PhysRegion {
    // SAFETY: each block is used as its owning task says; the mappings are just addresses.
    let be = unsafe { PhysMem::new() };
    be.map(base as u64, size).unwrap()
}

/// The GPIO controller. Every task may have its own handle: a level write touches only its pin, and the directions
/// are all set in `main`.
fn gpio() -> Gpio<PhysRegion> {
    Gpio::new(map(ps::map::GPIO_BASE, ps::map::GPIO_SIZE))
}

/// The reference clock of a controller, from the clock registers.
fn reference(read: impl Fn(&PhysRegion, &PhysRegion) -> u64) -> u32 {
    let crl = map(ps::map::CRL_APB_BASE, ps::map::CRL_APB_SIZE);
    let crf = map(ps::map::CRF_APB_BASE, ps::map::CRF_APB_SIZE);
    read(&crl, &crf) as u32
}

fn i2c(n: usize) -> I2c<PhysRegion> {
    let input = reference(|crl, crf| ps::clocks::i2c_ref_hz(crl, crf, PS_REF_HZ, n).unwrap());
    let i2c = I2c::new(map(
        [ps::map::I2C0_BASE, ps::map::I2C1_BASE][n],
        ps::map::I2C_SIZE,
    ));
    i2c.set_clock(input, 100_000).unwrap();
    i2c
}

fn spi(n: usize, mode: Mode) -> Spi<PhysRegion> {
    let input = reference(|crl, crf| ps::clocks::spi_ref_hz(crl, crf, PS_REF_HZ, n).unwrap());
    let spi = Spi::new(map(
        [ps::map::SPI0_BASE, ps::map::SPI1_BASE][n],
        ps::map::SPI_SIZE,
    ));
    spi.configure(mode, input, board::SPI_MAX_HZ).unwrap();
    spi
}

/// Record how far each release of a periodic task is from one period after the last.
fn lateness(last: &mut Instant, period: Duration, worst_ns: &AtomicU32) {
    let now = Instant::now();
    let gap = now.duration_since(*last);
    *last = now;
    worst_ns.fetch_max(gap.abs_diff(period).as_nanos() as u32, Relaxed);
}

fn heartbeat() {
    let gpio = gpio();
    let period = Duration::from_millis(250);
    let mut every = Periodic::every_ms(250);
    every.wait_next();
    let mut last = Instant::now();
    let mut on = false;
    loop {
        every.wait_next();
        lateness(&mut last, period, &HEARTBEAT_WORST_NS);
        on = !on;
        gpio.write(board::PS_LEDS[0], on).unwrap();
        BEATS.fetch_add(1, Relaxed);
    }
}

fn button() {
    let gpio = gpio();
    let period = Duration::from_millis(10);
    let mut every = Periodic::every_ms(10);
    every.wait_next();
    let mut last = Instant::now();
    let (mut before, mut pressed) = (false, false);
    loop {
        every.wait_next();
        lateness(&mut last, period, &BUTTON_WORST_NS);
        // Two looks 10 ms apart must agree: contacts bounce.
        let now = gpio.read(board::PS_BUTTON).unwrap();
        if now == before && now != pressed {
            pressed = now;
            gpio.write(board::PS_LEDS[1], pressed).unwrap();
            if pressed {
                let page = (PAGE.load(Relaxed) + 1) % PAGES;
                PAGE.store(page, Relaxed);
                REDRAW.give();
                say!("button: page {page}");
            }
        }
        before = now;
    }
}

fn monitor() {
    let mut bus = i2c(board::MONITOR_I2C);
    let die = map(ps::map::SYSMON_BASE, ps::map::SYSMON_SIZE);
    let mut every = Periodic::every_ms(500);
    loop {
        let started = Instant::now();
        // Talk to the chips with no lock held...
        let new = Readings {
            rails: board::RAILS.map(|r| ina220::read(&mut bus, r.address).ok()),
            temperature: sysmon::temperature(&die, Sensor::ProcessingSystem).unwrap(),
        };
        if new.rails.iter().any(Option::is_none) || new.temperature.is_none() {
            BAD_UPDATES.fetch_add(1, Relaxed);
        }
        // ...and lock only to hand the result over.
        *READINGS.lock(WAIT_FOREVER).unwrap() = new;
        UPDATES.fetch_add(1, Relaxed);
        MONITOR_LONGEST_US.fetch_max(started.elapsed().as_micros() as u32, Relaxed);
        every.wait_next();
    }
}

fn display() {
    let bus = spi(board::OLED_SPI, Mode::Mode3);
    let mut oled = Oled::new(bus.device(board::OLED_CHIP_SELECT));
    oled.init().unwrap();
    let mac = *MAC.lock(WAIT_FOREVER).unwrap();
    loop {
        let started = Instant::now();
        // A copy: the lock is not held while drawing, which takes milliseconds.
        let readings = *READINGS.lock(WAIT_FOREVER).unwrap();
        let lines: [Line; 2] = match PAGE.load(Relaxed) {
            0 => pages::identity(mac),
            1 => pages::temperatures([readings.temperature, None]),
            n => {
                let i = (n - 2) % board::RAILS.len();
                let none = Reading {
                    bus_millivolts: 0,
                    shunt_microvolts: 0,
                };
                pages::rail(board::RAILS[i], readings.rails[i].unwrap_or(none))
            }
        };
        for (row, l) in lines.iter().enumerate() {
            oled.write_line(row, l.as_str()).unwrap();
        }
        // The controller's address counter shows it took the second line whole.
        if oled.address().unwrap() != Some(0x50) {
            DISPLAY_FOLLOWS.store(false, Relaxed);
        }
        DRAWS.fetch_add(1, Relaxed);
        DRAW_LONGEST_US.fetch_max(started.elapsed().as_micros() as u32, Relaxed);
        REDRAW.take(500);
    }
}

fn clocks() {
    let started = Instant::now();
    let (mut lmk_words, mut lmx_words) = ([0; 160], [0; 128]);
    let lmk = tics::parse(LMK_FILE, &mut lmk_words).unwrap();
    let lmx = tics::parse(LMX_FILE, &mut lmx_words).unwrap();
    let bus = spi(board::CLOCK_SPI, Mode::Mode0);
    lmk04828::program(&mut bus.device(board::LMK_CHIP_SELECT), lmk).unwrap();
    delay_ms(100);
    let mut locked = true;
    for cs in [board::LMX_DAC_CHIP_SELECT, board::LMX_ADC_CHIP_SELECT] {
        let mut chip = bus.device(cs);
        lmx2594::program(&mut chip, lmx).unwrap();
        delay_ms(200);
        locked &= matches!(lmx2594::lock(&mut chip, lmx), Ok(lmx2594::Lock::Locked));
    }
    CLOCKS_LOCKED.store(locked, Relaxed);
    CLOCKS_MS.store(started.elapsed().as_millis() as u32, Relaxed);
    say!("clocks: programmed, both synthesizers locked: {locked}");
}

/// Each task's share of the processor and deepest stack use.
fn table() {
    let total = cpu_total_ticks().max(1);
    say!("  task        priority  processor  stack used");
    for slot in 0..task_slots() {
        if let Some(t) = task_info(slot) {
            let tenths = t.run_ticks * 1000 / total;
            say!(
                "  {:<10} {:>9} {:>8}.{}% {:>6} of {}",
                if slot == 0 { "(idle)" } else { t.name },
                t.priority,
                tenths / 10,
                tenths % 10,
                t.stack_used,
                t.stack_size
            );
        }
    }
}

fn check() {
    delay_ms(5000);
    let (beats, updates, draws) = (
        BEATS.load(Relaxed),
        UPDATES.load(Relaxed),
        DRAWS.load(Relaxed),
    );
    let (heart, buttons) = (
        HEARTBEAT_WORST_NS.load(Relaxed),
        BUTTON_WORST_NS.load(Relaxed),
    );
    say!("heartbeat: {beats} beats, worst {heart} ns from its time");
    say!("button: worst {buttons} ns from its time");
    say!(
        "monitor: {updates} updates ({} incomplete), longest {} us",
        BAD_UPDATES.load(Relaxed),
        MONITOR_LONGEST_US.load(Relaxed)
    );
    say!(
        "display: {draws} draws, longest {} us, follows: {}",
        DRAW_LONGEST_US.load(Relaxed),
        DISPLAY_FOLLOWS.load(Relaxed)
    );
    say!("clocks: done in {} ms", CLOCKS_MS.load(Relaxed));
    table();
    let good = beats >= 15
        && heart < 100_000
        && buttons < 100_000
        && updates >= 9
        && BAD_UPDATES.load(Relaxed) == 0
        && draws >= 8
        && DISPLAY_FOLLOWS.load(Relaxed)
        && CLOCKS_LOCKED.load(Relaxed);
    say!(
        "{}",
        if good {
            "RT BOARD OK"
        } else {
            "RT BOARD FAILED"
        }
    );
    say!("press PS_BTN to step through the display's pages");
}

fn main() -> ! {
    let tick = platform::init("RFSoC 4x2 board hardware on the kernel").tick_clock_hz;
    // Everything that changes a pin's direction happens here, before any task runs.
    let gpio = gpio();
    for pin in board::PS_LEDS {
        gpio.set_output(pin, false).unwrap();
    }
    gpio.set_input(board::PS_BUTTON).unwrap();
    // The clock chips: the reference select low, and the reset pin low for good (see `board::LMK_RESET`).
    for pin in board::LMK_CLOCK_SELECT {
        gpio.set_output(pin, false).unwrap();
    }
    gpio.set_output(board::LMK_RESET, false).unwrap();

    let mac =
        at24mac::mac_address(&mut i2c(board::IDENTITY_I2C), at24mac::IDENTITY_ADDRESS).unwrap();
    println!("Ethernet address {mac}");
    MAC.set(mac);
    READINGS.set(Readings {
        rails: [None; board::RAILS.len()],
        temperature: None,
    });

    spawn("heartbeat", 6, task_stack!(8192).unwrap(), heartbeat).unwrap();
    spawn("button", 5, task_stack!(8192).unwrap(), button).unwrap();
    spawn("monitor", 4, task_stack!(8192).unwrap(), monitor).unwrap();
    spawn("check", 3, task_stack!(16384).unwrap(), check).unwrap();
    spawn("display", 2, task_stack!(16384).unwrap(), display).unwrap();
    spawn("clocks", 1, task_stack!(16384).unwrap(), clocks).unwrap();
    start(tick, 1000)
}
