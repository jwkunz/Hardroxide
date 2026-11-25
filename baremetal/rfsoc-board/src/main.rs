//! The RFSoC 4x2's processor-side hardware, on bare metal: the board's identity, its die temperatures and nine power
//! monitors, the OLED display, the radio clock chips, and the processor's LEDs and push button.
//!
//! It checks each in turn and prints `BOARD DEMO OK`, then keeps running: the button steps the display through its
//! pages, one LED blinks and the other follows the button.
//!
//! Nothing is written to any memory that survives power-off. The clock chips are programmed (they forget at power-off)
//! with the register files in `fixtures/rfsoc4x2/clocks`, which give the converters their 491.52 MHz references.

#![no_std]
#![no_main]

use core::fmt::Write;
use core::time::Duration;
use hardroxide_board::rfsoc4x2 as board;
use hardroxide_board::us2066::{COLUMNS, Oled};
use hardroxide_board::{at24mac, ina220, lmk04828, lmx2594, tics};
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use platform::ps::{self, println};
use ps::gpio::Gpio;
use ps::i2c::I2c;
use ps::spi::{Device, Mode, Spi};
use ps::sysmon::{self, Sensor};

#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;

ps::entry!(main);

const PS_REF_HZ: u64 = 33_333_333;
const LMK_FILE: &str = include_str!("../../../fixtures/rfsoc4x2/clocks/LMK04828_245.76.txt");
const LMX_FILE: &str = include_str!("../../../fixtures/rfsoc4x2/clocks/LMX2594_491.52.txt");

/// A line of the display, written to with `write!`; surplus characters are dropped.
#[derive(Default)]
struct Line {
    text: [u8; COLUMNS],
    len: usize,
}

impl Line {
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.text[..self.len]).unwrap_or("?")
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes().take(COLUMNS - self.len) {
            self.text[self.len] = b;
            self.len += 1;
        }
        Ok(())
    }
}

macro_rules! line {
    ($($arg:tt)*) => {{
        let mut l = Line::default();
        let _ = write!(l, $($arg)*);
        l
    }};
}

/// Counts what did not behave.
struct Checks(u32);

impl Checks {
    fn check(&mut self, what: &str, good: bool) {
        if !good {
            println!("  FAILED: {what}");
            self.0 += 1;
        }
    }
}

fn map(base: usize, size: usize) -> PhysRegion {
    // SAFETY: each block mapped here is used by this program alone (the clock registers are only read).
    let be = unsafe { PhysMem::new() };
    be.map(base as u64, size).unwrap()
}

fn i2c(n: usize) -> I2c<PhysRegion> {
    let (crl, crf) = (
        map(ps::map::CRL_APB_BASE, ps::map::CRL_APB_SIZE),
        map(ps::map::CRF_APB_BASE, ps::map::CRF_APB_SIZE),
    );
    let input = ps::clocks::i2c_ref_hz(&crl, &crf, PS_REF_HZ, n).unwrap() as u32;
    let i2c = I2c::new(map(
        [ps::map::I2C0_BASE, ps::map::I2C1_BASE][n],
        ps::map::I2C_SIZE,
    ));
    i2c.set_clock(input, 100_000).unwrap();
    i2c
}

fn spi(n: usize, mode: Mode) -> Spi<PhysRegion> {
    let (crl, crf) = (
        map(ps::map::CRL_APB_BASE, ps::map::CRL_APB_SIZE),
        map(ps::map::CRF_APB_BASE, ps::map::CRF_APB_SIZE),
    );
    let input = ps::clocks::spi_ref_hz(&crl, &crf, PS_REF_HZ, n).unwrap() as u32;
    let spi = Spi::new(map(
        [ps::map::SPI0_BASE, ps::map::SPI1_BASE][n],
        ps::map::SPI_SIZE,
    ));
    let hz = spi.configure(mode, input, board::SPI_MAX_HZ).unwrap();
    println!("  SPI {n} at {hz} Hz");
    spi
}

/// Millidegrees as `41.2`.
struct Degrees(i32);

impl core::fmt::Display for Degrees {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let tenths = (self.0 + 50).div_euclid(100);
        write!(f, "{}.{}", tenths / 10, (tenths % 10).abs())
    }
}

fn identity(c: &mut Checks) -> at24mac::MacAddress {
    println!("identity (AT24MAC402 on I2C {}):", board::IDENTITY_I2C);
    let mut bus = i2c(board::IDENTITY_I2C);
    let mac = at24mac::mac_address(&mut bus, at24mac::IDENTITY_ADDRESS).unwrap();
    let serial = at24mac::serial_number(&mut bus, at24mac::IDENTITY_ADDRESS).unwrap();
    let again = at24mac::mac_address(&mut bus, at24mac::IDENTITY_ADDRESS).unwrap();
    println!("  Ethernet address {mac}");
    ps::print!("  serial number ");
    for b in serial {
        ps::print!("{b:02x}");
    }
    println!();
    c.check("the Ethernet address is a station's", mac.is_unicast());
    c.check("it reads the same twice", mac == again);
    c.check(
        "the serial number is not blank",
        serial != [0; 16] && serial != [0xFF; 16],
    );
    mac
}

/// Both die temperatures, in thousandths of a degree.
fn temperatures() -> [Option<i32>; 2] {
    let regs = map(ps::map::SYSMON_BASE, ps::map::SYSMON_SIZE);
    [Sensor::ProcessingSystem, Sensor::ProgrammableLogic]
        .map(|s| sysmon::temperature(&regs, s).unwrap())
}

fn temperature_check(c: &mut Checks) {
    println!("die temperatures:");
    for (name, value) in ["processing system", "programmable logic"]
        .iter()
        .zip(temperatures())
    {
        match value {
            Some(t) => println!("  {name}: {} C", Degrees(t)),
            None => println!("  {name}: no reading"),
        }
        c.check(
            "the temperature is plausible",
            value.is_some_and(|t| (10_000..100_000).contains(&t)),
        );
    }
}

fn power(c: &mut Checks, bus: &mut I2c<PhysRegion>) {
    println!("power monitors (INA220 on I2C {}):", board::MONITOR_I2C);
    let mut total = 0;
    for rail in board::RAILS {
        let configuration = ina220::configuration(bus, rail.address).unwrap();
        let r = ina220::read(bus, rail.address).unwrap();
        let (v, ma, mw) = (
            r.bus_millivolts,
            r.milliamps(rail.shunt_micro_ohms),
            r.milliwatts(rail.shunt_micro_ohms),
        );
        println!("  {:<13} {v:>5} mV {ma:>6} mA {mw:>6} mW", rail.name);
        total += mw;
        c.check(
            "the monitor is in its reset configuration",
            configuration == ina220::RESET_CONFIGURATION,
        );
        // The three rails named after their voltage, within 5 %.
        let nominal = match rail.name {
            "VCC0V85" => Some(850),
            "VCC3V3" => Some(3300),
            "VCC1V8" => Some(1800),
            _ => None,
        };
        if let Some(n) = nominal {
            c.check(
                "the rail is within 5 % of its name",
                v.abs_diff(n) <= n / 20,
            );
        }
    }
    println!("  {total} mW through the nine monitored rails");
}

/// Set the display up and check, by reading its address counter back, that it follows.
fn display<'a>(c: &mut Checks, bus: &'a Spi<PhysRegion>) -> Oled<Device<'a, PhysRegion>> {
    let mut oled = Oled::new(bus.device(board::OLED_CHIP_SELECT));
    oled.init().unwrap();
    oled.write_line(0, "Hardroxide").unwrap();
    let after_first = oled.address().unwrap();
    oled.write_line(1, "RFSoC 4x2").unwrap();
    let after_second = oled.address().unwrap();
    oled.set_address(0x25).unwrap();
    let moved = oled.address().unwrap();
    println!("  address counter read back: {after_first:x?}, {after_second:x?}, {moved:x?}");
    c.check(
        "the display's address counter follows the writes",
        (after_first, after_second, moved) == (Some(0x10), Some(0x50), Some(0x25)),
    );
    oled
}

/// Program both synthesizers and report whether each locks; `expect` says what to require of the DAC's and the ADC's.
fn synthesizers(c: &mut Checks, bus: &Spi<PhysRegion>, lmx: &[u32], expect: [Option<bool>; 2]) {
    for ((name, cs), expect_lock) in [
        ("DAC", board::LMX_DAC_CHIP_SELECT),
        ("ADC", board::LMX_ADC_CHIP_SELECT),
    ]
    .into_iter()
    .zip(expect)
    {
        let mut chip = bus.device(cs);
        lmx2594::program(&mut chip, lmx).unwrap();
        sleep(Duration::from_millis(200));
        let state = lmx2594::lock(&mut chip, lmx);
        match &state {
            Ok(l) => println!("  LMX2594 ({name}): {l:?}"),
            Err(e) => println!("  LMX2594 ({name}): {e}"),
        }
        c.check(
            "the synthesizer answers, and its lock is as expected",
            state.is_ok_and(|l| expect_lock.is_none_or(|e| (l == lmx2594::Lock::Locked) == e)),
        );
    }
}

fn clocks(c: &mut Checks, gpio: &Gpio<PhysRegion>) {
    println!("clock chips:");
    let (mut lmk_words, mut lmx_words) = ([0; 160], [0; 128]);
    let lmk = tics::parse(LMK_FILE, &mut lmk_words).unwrap();
    let lmx = tics::parse(LMX_FILE, &mut lmx_words).unwrap();
    let bus = spi(board::CLOCK_SPI, Mode::Mode0);
    for pin in board::LMK_CLOCK_SELECT {
        gpio.set_output(pin, false).unwrap();
    }
    // The LMK04828's reset pin is held low, never driven high (see `board::LMK_RESET`): the chip is reset over SPI.
    gpio.set_output(board::LMK_RESET, false).unwrap();
    // The synthesizers' references come from the LMK04828. Freshly reset, it gives the DAC's none, so that one must
    // not lock, which shows that the lock readout can say no. (The ADC's still locks on this board, so some clock
    // reaches it even then.)
    lmk04828::reset(&mut bus.device(board::LMK_CHIP_SELECT)).unwrap();
    sleep(Duration::from_millis(10));
    println!("  with the LMK04828 reset to its power-on state:");
    synthesizers(c, &bus, lmx, [Some(false), None]);
    // Once the LMK04828 is programmed, both must lock.
    lmk04828::program(&mut bus.device(board::LMK_CHIP_SELECT), lmk).unwrap();
    sleep(Duration::from_millis(100));
    println!("  LMK04828 programmed ({} writes):", lmk.len());
    synthesizers(c, &bus, lmx, [Some(true), Some(true)]);
}

fn leds_and_button(c: &mut Checks, gpio: &Gpio<PhysRegion>) {
    println!("processor LEDs and button:");
    for pin in board::PS_LEDS {
        gpio.set_output(pin, false).unwrap();
    }
    gpio.set_input(board::PS_BUTTON).unwrap();
    for pattern in [1, 2, 3, 0] {
        for (n, pin) in board::PS_LEDS.into_iter().enumerate() {
            gpio.write(pin, pattern & (1 << n) != 0).unwrap();
        }
        sleep(Duration::from_millis(250));
        for (n, pin) in board::PS_LEDS.into_iter().enumerate() {
            c.check(
                "the LED pin is at the level written",
                gpio.read(pin).unwrap() == (pattern & (1 << n) != 0),
            );
        }
    }
    println!(
        "  button: {}",
        if gpio.read(board::PS_BUTTON).unwrap() {
            "pressed"
        } else {
            "released"
        }
    );
}

/// What page `page` of the display shows.
fn page(page: usize, mac: at24mac::MacAddress, bus: &mut I2c<PhysRegion>) -> [Line; 2] {
    match page {
        0 => {
            let [a, b, c, d, e, f] = mac.0;
            [
                line!("RFSoC 4x2"),
                line!("MAC {a:02x}{b:02x}{c:02x}{d:02x}{e:02x}{f:02x}"),
            ]
        }
        1 => {
            let [ps_die, pl_die] = temperatures().map(|t| Degrees(t.unwrap_or(0)));
            [line!("PS die  {ps_die} C"), line!("PL die  {pl_die} C")]
        }
        n => {
            let rail = board::RAILS[(n - 2) % board::RAILS.len()];
            let r = ina220::read(bus, rail.address).unwrap();
            let v = r.bus_millivolts;
            let ma = r.milliamps(rail.shunt_micro_ohms).max(0);
            let mw = r.milliwatts(rail.shunt_micro_ohms).max(0);
            [
                line!("{}", rail.name),
                line!(
                    "{}.{:02}V {}.{:02}A {}.{}W",
                    v / 1000,
                    v % 1000 / 10,
                    ma / 1000,
                    ma % 1000 / 10,
                    mw / 1000,
                    mw % 1000 / 100
                ),
            ]
        }
    }
}

const PAGES: usize = 2 + board::RAILS.len();

fn main() -> ! {
    platform::init("RFSoC 4x2 board demo");
    let mut c = Checks(0);
    let gpio = Gpio::new(map(ps::map::GPIO_BASE, ps::map::GPIO_SIZE));

    let mac = identity(&mut c);
    temperature_check(&mut c);
    let mut monitors = i2c(board::MONITOR_I2C);
    power(&mut c, &mut monitors);
    println!("OLED display:");
    let oled_bus = spi(board::OLED_SPI, Mode::Mode3);
    let mut oled = display(&mut c, &oled_bus);
    clocks(&mut c, &gpio);
    leds_and_button(&mut c, &gpio);

    if c.0 == 0 {
        println!("BOARD DEMO OK");
    } else {
        println!("BOARD DEMO FAILED: {} checks", c.0);
    }
    println!("press PS_BTN to step through the display's pages");

    let (mut current, mut was_pressed, mut shown) = (0, false, None::<Instant>);
    let start = Instant::now();
    loop {
        let pressed = gpio.read(board::PS_BUTTON).unwrap();
        if pressed && !was_pressed {
            current = (current + 1) % PAGES;
            shown = None;
        }
        was_pressed = pressed;
        // Redraw on a new page, and every half second for fresh readings.
        if shown.is_none_or(|t| t.elapsed() > Duration::from_millis(500)) {
            let first = shown.is_none();
            let lines = page(current, mac, &mut monitors);
            for (row, l) in lines.iter().enumerate() {
                oled.write_line(row, l.as_str()).unwrap();
            }
            if first {
                println!(
                    "page {current}: [{:<16}] [{:<16}]",
                    lines[0].as_str(),
                    lines[1].as_str()
                );
            }
            shown = Some(Instant::now());
        }
        let beat = (start.elapsed().as_millis() / 500).is_multiple_of(2);
        gpio.write(board::PS_LEDS[0], beat).unwrap();
        gpio.write(board::PS_LEDS[1], pressed).unwrap();
        sleep(Duration::from_millis(20));
    }
}
