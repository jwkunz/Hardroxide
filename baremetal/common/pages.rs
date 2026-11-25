// The text the board programs show on the RFSoC 4x2's 16x2 display, shared by `rfsoc-board` and `rt-board`.

use core::fmt::Write;
use hardroxide_board::at24mac::MacAddress;
use hardroxide_board::ina220::Reading;
use hardroxide_board::rfsoc4x2::{RAILS, Rail};
use hardroxide_board::us2066::COLUMNS;

/// Identity, die temperatures, then one page for each monitored supply.
pub const PAGES: usize = 2 + RAILS.len();

/// A line of the display, written to with `write!`; surplus characters are dropped.
#[derive(Default)]
pub struct Line {
    text: [u8; COLUMNS],
    len: usize,
}

impl Line {
    pub fn as_str(&self) -> &str {
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

/// Millidegrees as `41.2`.
pub struct Degrees(pub i32);

impl core::fmt::Display for Degrees {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let tenths = (self.0 + 50).div_euclid(100);
        write!(f, "{}.{}", tenths / 10, (tenths % 10).abs())
    }
}

pub fn identity(mac: MacAddress) -> [Line; 2] {
    let [a, b, c, d, e, f] = mac.0;
    [
        line!("RFSoC 4x2"),
        line!("MAC {a:02x}{b:02x}{c:02x}{d:02x}{e:02x}{f:02x}"),
    ]
}

/// The processing system's and the FPGA's temperatures, in thousandths of a degree.
pub fn temperatures([ps, pl]: [Option<i32>; 2]) -> [Line; 2] {
    let shown = |name, t: Option<i32>| match t {
        Some(t) => line!("{name} die  {} C", Degrees(t)),
        None => line!("{name} die  --"),
    };
    [shown("PS", ps), shown("PL", pl)]
}

pub fn rail(rail: Rail, r: Reading) -> [Line; 2] {
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
