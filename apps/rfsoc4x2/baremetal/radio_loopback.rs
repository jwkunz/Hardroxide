//! RFSoC 4x2 base overlay, bare metal: send a tone from each DAC, record it with an ADC and find it in the spectrum.
//!
//! SMA cables from DAC A to ADC A and from DAC B to ADC B. The program programs the board's clock chips (they forget
//! at power-off; nothing else is written that outlasts it), starts the converters and prints one line per check, then
//! `RADIO LOOPBACK OK`.
//!
//! Beside the generated crate's own, it needs `hardroxide-board` as a development dependency, the clock chips'
//! register files (`fixtures/rfsoc4x2/clocks/*.txt`) next to this file, and `common/loopback.rs` in `common/`.
#![no_std]
#![no_main]

use core::ptr::addr_of_mut;
use core::time::Duration;
use design_1::{Design, halt, psu};
use hardroxide_board::rfsoc4x2 as board;
use hardroxide_board::{lmk04828, lmx2594, tics};
use hardroxide_dma::static_dma_buffer;
use hardroxide_mmio::time::sleep;
use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use psu::gpio::Gpio;
use psu::println;
use psu::spi::{Mode, Spi};

#[path = "common/loopback.rs"]
mod loopback;
use loopback::{BUFFER_BYTES, Bench, Receiver, SAMPLES, Spectrum, Transmitter};

psu::entry!(main);

const PS_REF_HZ: u64 = 33_333_333;
const LMK_FILE: &str = include_str!("LMK04828_245.76.txt");
const LMX_FILE: &str = include_str!("LMX2594_491.52.txt");

/// The spectrum's working memory: too large for the stack.
static mut RE: [f64; SAMPLES] = [0.0; SAMPLES];
static mut IM: [f64; SAMPLES] = [0.0; SAMPLES];

fn map(base: usize, size: usize) -> PhysRegion {
    // SAFETY: each block mapped here is used by this program alone (the clock registers are only read).
    let be = unsafe { PhysMem::new() };
    be.map(base as u64, size).unwrap()
}

/// Program the LMK04828 and, from its outputs, the two synthesizers that clock the converters. Whether both lock.
fn clocks() -> bool {
    let (mut lmk_words, mut lmx_words) = ([0; 160], [0; 128]);
    let lmk = tics::parse(LMK_FILE, &mut lmk_words).unwrap();
    let lmx = tics::parse(LMX_FILE, &mut lmx_words).unwrap();
    let (crl, crf) = (
        map(psu::map::CRL_APB_BASE, psu::map::CRL_APB_SIZE),
        map(psu::map::CRF_APB_BASE, psu::map::CRF_APB_SIZE),
    );
    let input = psu::clocks::spi_ref_hz(&crl, &crf, PS_REF_HZ, board::CLOCK_SPI).unwrap() as u32;
    let bus = Spi::new(map(
        [psu::map::SPI0_BASE, psu::map::SPI1_BASE][board::CLOCK_SPI],
        psu::map::SPI_SIZE,
    ));
    bus.configure(Mode::Mode0, input, board::SPI_MAX_HZ)
        .unwrap();
    let gpio = Gpio::new(map(psu::map::GPIO_BASE, psu::map::GPIO_SIZE));
    for pin in board::LMK_CLOCK_SELECT {
        gpio.set_output(pin, false).unwrap();
    }
    // The reset pin is held low, never driven high (see `board::LMK_RESET`): the chip is reset over SPI.
    gpio.set_output(board::LMK_RESET, false).unwrap();
    lmk04828::reset(&mut bus.device(board::LMK_CHIP_SELECT)).unwrap();
    sleep(Duration::from_millis(10));
    lmk04828::program(&mut bus.device(board::LMK_CHIP_SELECT), lmk).unwrap();
    sleep(Duration::from_millis(100));
    let mut locked = true;
    for (name, select) in [
        ("DAC", board::LMX_DAC_CHIP_SELECT),
        ("ADC", board::LMX_ADC_CHIP_SELECT),
    ] {
        let mut chip = bus.device(select);
        lmx2594::program(&mut chip, lmx).unwrap();
        sleep(Duration::from_millis(200));
        let state = lmx2594::lock(&mut chip, lmx);
        match &state {
            Ok(l) => println!("{name} synthesizer: {l:?}"),
            Err(e) => println!("{name} synthesizer: {e}"),
        }
        locked &= state.is_ok_and(|l| l == lmx2594::Lock::Locked);
    }
    locked
}

fn main() -> ! {
    let mut d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    println!("\n== RFSoC 4x2 base overlay: radio loopback ==");
    if !clocks() {
        println!("RADIO LOOPBACK FAILED: the clock chips did not lock");
        halt()
    }
    let buffers = (
        static_dma_buffer!(BUFFER_BYTES).unwrap(),
        static_dma_buffer!(BUFFER_BYTES).unwrap(),
    );
    // SAFETY: this is the only reference ever made to either array.
    let (re, im) = unsafe { (&mut *addr_of_mut!(RE), &mut *addr_of_mut!(IM)) };

    macro_rules! receiver {
        ($name:literal, $tile:literal, $block:literal, $generator:ident, $i:ident, $q:ident) => {
            Receiver {
                name: $name,
                tile: $tile,
                block: $block,
                generator: &mut d.$generator,
                i: &d.$i,
                q: &d.$q,
            }
        };
    }
    let mut print = |text: core::fmt::Arguments| psu::print!("{text}");
    let failures = loopback::measure(Bench {
        rfdc: &mut d.radio_rfdc,
        transmitters: [
            Transmitter {
                name: "DAC A",
                tile: 2,
                amplitude: &mut d.radio_transmitter_channel_20_amplitude_controller,
            },
            Transmitter {
                name: "DAC B",
                tile: 0,
                amplitude: &mut d.radio_transmitter_channel_00_amplitude_controller,
            },
        ],
        receivers: [
            receiver!(
                "ADC A (receiver 21)",
                2,
                1,
                radio_receiver_channel_21_packet_generator,
                radio_receiver_channel_21_axi_dma_real,
                radio_receiver_channel_21_axi_dma_imag
            ),
            receiver!(
                "ADC B (receiver 20)",
                2,
                0,
                radio_receiver_channel_20_packet_generator,
                radio_receiver_channel_20_axi_dma_real,
                radio_receiver_channel_20_axi_dma_imag
            ),
            receiver!(
                "receiver 00",
                0,
                0,
                radio_receiver_channel_00_packet_generator,
                radio_receiver_channel_00_axi_dma_real,
                radio_receiver_channel_00_axi_dma_imag
            ),
            receiver!(
                "receiver 01",
                0,
                1,
                radio_receiver_channel_01_packet_generator,
                radio_receiver_channel_01_axi_dma_real,
                radio_receiver_channel_01_axi_dma_imag
            ),
        ],
        buffers: Some(buffers),
        spectrum: Spectrum::new(re, im),
        print: &mut print,
        failures: 0,
    });
    if failures == 0 {
        println!("RADIO LOOPBACK OK");
    } else {
        println!("RADIO LOOPBACK FAILED: {failures} check(s)");
    }
    halt()
}
