//! RFSoC 4x2 base overlay, bare metal: send a tone from each DAC, record it with an ADC and find it in the spectrum.
//!
//! SMA cables from DAC A to ADC A and from DAC B to ADC B. The program programs the board's clock chips (they forget
//! at power-off; nothing else is written that outlasts it), starts the converters and prints one line per check, then
//! `RADIO LOOPBACK OK`.
//!
//! Beside the generated crate's own, it needs `hardroxide-board` as a development dependency, the clock chips'
//! register files (`fixtures/rfsoc4x2/clocks/*.txt`) next to this file, and `common/loopback.rs` and
//! `common/clocks.rs` in `common/`.
#![no_std]
#![no_main]

use core::ptr::addr_of_mut;
use design_1::{Design, halt, psu};
use hardroxide_dma::static_dma_buffer;
use psu::println;

#[path = "common/clocks.rs"]
mod clocks;
#[path = "common/loopback.rs"]
mod loopback;
use loopback::{BUFFER_BYTES, SAMPLES, Spectrum};

psu::entry!(main);

/// The spectrum's working memory: too large for the stack.
static mut RE: [f64; SAMPLES] = [0.0; SAMPLES];
static mut IM: [f64; SAMPLES] = [0.0; SAMPLES];

fn main() -> ! {
    let mut d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== RFSoC 4x2 base overlay: radio loopback ==");
    if !clocks::program() {
        println!("RADIO LOOPBACK FAILED: the clock chips did not lock");
        halt()
    }
    let buffers = (
        static_dma_buffer!(BUFFER_BYTES).unwrap(),
        static_dma_buffer!(BUFFER_BYTES).unwrap(),
    );
    // SAFETY: this is the only reference ever made to either array.
    let (re, im) = unsafe { (&mut *addr_of_mut!(RE), &mut *addr_of_mut!(IM)) };
    let mut print = |text: core::fmt::Arguments| psu::print!("{text}");
    let failures = loopback::measure(loopback::base_overlay_bench!(
        d,
        buffers,
        Spectrum::new(re, im),
        &mut print
    ));
    if failures == 0 {
        println!("RADIO LOOPBACK OK");
    } else {
        println!("RADIO LOOPBACK FAILED: {failures} check(s)");
    }
    halt()
}
