//! A process-wide console on a PS UART, for `print!`/`println!` and for the exception
//! and panic reports.
//!
//! [`init`] records which UART to use; writing is then possible from any context. There
//! is no locking: output from an interrupt handler and the main program can interleave,
//! which is acceptable for diagnostics (and memory-safe, since the UART is only ever
//! used through shared references).

use crate::uart::Uart;
use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};

static BASE: AtomicUsize = AtomicUsize::new(0);

fn uart() -> Option<Uart<PhysRegion>> {
    let base = BASE.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    // SAFETY: `init`'s contract: `base` is a UART register window used only through here.
    let be = unsafe { PhysMem::new() };
    be.map(base as u64, crate::map::UART_SIZE)
        .ok()
        .map(Uart::new)
}

/// Initialise the UART at `base` for 8N1 at `baud` and make it the console. Returns the
/// baud rate actually produced, or `None` if it could not be set (the console is then
/// left unset).
///
/// # Safety
/// `base` must be a UART's register window (`UART0_BASE`/`UART1_BASE`), already clocked
/// and pin-muxed by `ps7_init`, and not used by other code.
pub unsafe fn init(base: usize, ref_clk_hz: u32, baud: u32) -> Option<u32> {
    // SAFETY: as documented above.
    let be = unsafe { PhysMem::new() };
    let u = Uart::new(be.map(base as u64, crate::map::UART_SIZE).ok()?);
    let actual = u.init(ref_clk_hz, baud).ok()??;
    BASE.store(base, Ordering::Release);
    Some(actual)
}

/// Whether a console has been set up.
pub fn is_initialised() -> bool {
    BASE.load(Ordering::Acquire) != 0
}

struct Out<'a>(&'a Uart<PhysRegion>);

impl fmt::Write for Out<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' && !matches!(self.0.write_byte(b'\r'), Ok(true)) {
                return Err(fmt::Error);
            }
            if !matches!(self.0.write_byte(b), Ok(true)) {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

/// Write formatted text to the console; silently does nothing if there is none.
pub fn write_fmt(args: fmt::Arguments<'_>) {
    if let Some(u) = uart() {
        let _ = fmt::write(&mut Out(&u), args);
    }
}

/// Print to the console.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::console::write_fmt(core::format_args!($($arg)*)) };
}

/// Print a line to the console.
#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", core::format_args!($($arg)*)) };
}
