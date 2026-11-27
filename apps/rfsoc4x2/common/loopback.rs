//! The radio loopback measurement of the RFSoC 4x2 base overlay, shared by the bare-metal and Linux programs
//! (`radio_loopback.rs`): send tones from the two DACs, record the four receivers, find the tones in the spectrum.
//!
//! Wire SMA cables from DAC A to ADC A and from DAC B to ADC B. The clock chips must be programmed before
//! [`measure`] is called: it starts the converters' tiles, which fails without clocks, before it touches any block
//! that would not answer without them.

use core::fmt::Arguments;
use core::time::Duration;
use hardroxide_dma::DmaBuffer;
use hardroxide_ip::dma::AxiDma;
use hardroxide_ip::radio::{self, AmplitudeController, PacketGenerator};
use hardroxide_ip::rfdc::{Converter, Mixer, Rfdc};
use hardroxide_mmio::Mmio;
use hardroxide_mmio::time::sleep;

/// Samples in one record, I and Q each. Half of the most the packet generator sends: in RFSoC-PYNQ's bitstream,
/// receiver 20 has no data buffer on its way to memory and loses the end of records longer than about 30000 samples.
pub const SAMPLES: usize = 16384;
/// Room each of the two buffers needs.
pub const BUFFER_BYTES: usize = 2 * SAMPLES;
/// The receivers' rate after the converters' decimation by two.
const RATE_HZ: i64 = 2_457_600_000;
/// The width of one bin of a record's spectrum: 150 kHz.
const BIN_HZ: i64 = RATE_HZ / SAMPLES as i64;
const AMPLITUDE: u16 = 0x4000;

pub struct Transmitter<'a, R: Mmio> {
    pub name: &'static str,
    /// The DAC tile; each has one block, number 0.
    pub tile: usize,
    pub amplitude: &'a mut AmplitudeController<R>,
}

pub struct Receiver<'a, R: Mmio> {
    pub name: &'static str,
    pub tile: usize,
    pub block: usize,
    pub generator: &'a mut PacketGenerator<R>,
    pub i: &'a AxiDma<R>,
    pub q: &'a AxiDma<R>,
}

/// The sine and cosine of an angle of at most pi either way, by their series (there is no maths library here).
fn cos_sin(x: f64) -> (f64, f64) {
    let (mut cos, mut sin, mut term) = (0.0, 0.0, 1.0);
    for n in 0..30 {
        match n % 4 {
            0 => cos += term,
            1 => sin += term,
            2 => cos -= term,
            _ => sin -= term,
        }
        term *= x / f64::from(n + 1);
    }
    (cos, sin)
}

/// The Fourier transform of a power-of-two number of complex samples, in place.
fn transform(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let (c, s) = cos_sin(core::f64::consts::TAU / length as f64);
        for start in (0..n).step_by(length) {
            let (mut wr, mut wi) = (1.0, 0.0);
            for k in start..start + length / 2 {
                let m = k + length / 2;
                let (tr, ti) = (re[m] * wr - im[m] * wi, re[m] * wi + im[m] * wr);
                (re[m], im[m]) = (re[k] - tr, im[k] - ti);
                (re[k], im[k]) = (re[k] + tr, im[k] + ti);
                (wr, wi) = (wr * c + wi * s, wi * c - wr * s);
            }
        }
        length *= 2;
    }
}

/// Ten times the base-ten logarithm of a power: decibels. Very small powers are reported as -200.
fn decibels(power: f64) -> f64 {
    if power < 1e-20 {
        return -200.0;
    }
    let bits = power.to_bits();
    let exponent = (bits >> 52) as i64 - 1023;
    let mantissa = f64::from_bits(bits & 0x000F_FFFF_FFFF_FFFF | 0x3FF0_0000_0000_0000);
    let z = (mantissa - 1.0) / (mantissa + 1.0);
    let (mut term, mut sum) = (z, 0.0);
    for n in 0..20 {
        sum += term / f64::from(2 * n + 1);
        term *= z * z;
    }
    let natural = 2.0 * sum + exponent as f64 * core::f64::consts::LN_2;
    10.0 * natural / core::f64::consts::LN_10
}

/// What a record's spectrum shows.
pub struct Spectrum<'a> {
    re: &'a mut [f64; SAMPLES],
    im: &'a mut [f64; SAMPLES],
}

impl<'a> Spectrum<'a> {
    pub fn new(re: &'a mut [f64; SAMPLES], im: &'a mut [f64; SAMPLES]) -> Self {
        Self { re, im }
    }

    fn compute(&mut self, i: &[u8], q: &[u8]) {
        let sample = |bytes: &[u8], n: usize| {
            f64::from(i16::from_le_bytes([bytes[2 * n], bytes[2 * n + 1]]))
        };
        for n in 0..SAMPLES {
            // Full scale is 1.
            self.re[n] = sample(i, n) / 32768.0;
            self.im[n] = sample(q, n) / 32768.0;
        }
        transform(self.re, self.im);
    }

    /// The level at a frequency, in dB relative to full scale: a full-scale tone there reads 0.
    fn level(&self, hz: i64) -> f64 {
        let bin = (hz / BIN_HZ).rem_euclid(SAMPLES as i64) as usize;
        let scale = 1.0 / (SAMPLES as f64 * SAMPLES as f64);
        decibels((self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin]) * scale)
    }

    /// The frequency of the strongest bin, negative below zero, leaving out the bin at `except_hz` and its
    /// neighbours.
    fn peak_except(&self, except_hz: i64) -> i64 {
        let power = |k: usize| self.re[k] * self.re[k] + self.im[k] * self.im[k];
        let except = (except_hz / BIN_HZ).rem_euclid(SAMPLES as i64) as usize;
        let mut best = None;
        for k in 0..SAMPLES {
            let near = (k + SAMPLES + 1 - except) % SAMPLES <= 2;
            if !near && best.is_none_or(|b| power(k) > power(b)) {
                best = Some(k);
            }
        }
        let best = best.expect("more bins than are left out") as i64;
        let signed = if best < SAMPLES as i64 / 2 {
            best
        } else {
            best - SAMPLES as i64
        };
        signed * BIN_HZ
    }
}

/// Where a frequency appears among the receivers' samples: within half their rate either way.
fn wrap(hz: i64) -> i64 {
    (hz + RATE_HZ / 2).rem_euclid(RATE_HZ) - RATE_HZ / 2
}

/// Everything the measurement uses.
pub struct Bench<'a, R: Mmio, B: DmaBuffer> {
    pub rfdc: &'a mut Rfdc<R>,
    /// DAC A, then DAC B.
    pub transmitters: [Transmitter<'a, R>; 2],
    /// ADC A and ADC B first (cabled to DAC A and DAC B), then the other two.
    pub receivers: [Receiver<'a, R>; 4],
    /// For I and for Q, [`BUFFER_BYTES`] each.
    pub buffers: Option<(B, B)>,
    pub spectrum: Spectrum<'a>,
    pub print: &'a mut dyn FnMut(Arguments),
    pub failures: u32,
}

impl<R: Mmio, B: DmaBuffer> Bench<'_, R, B> {
    fn check(&mut self, good: bool, what: Arguments) {
        let verdict = if good { "ok  " } else { "FAIL" };
        (self.print)(format_args!("  {verdict} {what}\n"));
        self.failures += u32::from(!good);
    }

    fn record(&mut self, receiver: usize) -> Result<(), radio::RadioError> {
        let r = &mut self.receivers[receiver];
        let buffers = self
            .buffers
            .take()
            .expect("the buffers, back from the last record");
        let (i, q) = radio::record(
            r.generator,
            r.i,
            r.q,
            buffers,
            SAMPLES,
            Duration::from_millis(100),
        )?;
        self.spectrum.compute(i.as_slice(), q.as_slice());
        self.buffers = Some((i, q));
        Ok(())
    }

    fn tune(
        &mut self,
        converter: Converter,
        tile: usize,
        block: usize,
        hz: i64,
    ) -> Result<(), hardroxide_ip::rfdc::RfdcError> {
        let mixer = Mixer {
            frequency_hz: hz,
            phase_degrees: 0.0,
        };
        self.rfdc.set_mixer(converter, tile, block, mixer)
    }
}

/// An error of any driver, as text through `print`.
macro_rules! attempt {
    ($bench:ident, $what:expr, $result:expr) => {
        match $result {
            Ok(v) => v,
            Err(e) => {
                ($bench.print)(format_args!("  FAIL {}: {e}\n", $what));
                $bench.failures += 1;
                return $bench.failures;
            }
        }
    };
}

/// Run the whole measurement and return the number of checks that failed. An error stops it.
pub fn measure<R: Mmio, B: DmaBuffer>(mut b: Bench<'_, R, B>) -> u32 {
    (b.print)(format_args!("converter tiles:\n"));
    // First, and all of them: the other radio blocks run on the converters' clocks.
    for (converter, tile) in [
        (Converter::Dac, 0),
        (Converter::Dac, 2),
        (Converter::Adc, 0),
        (Converter::Adc, 2),
    ] {
        let status = attempt!(b, "starting a tile", b.rfdc.start_tile(converter, tile));
        b.check(
            status.running() && status.pll_locked,
            format_args!(
                "{converter} tile {tile}: state {}, PLL locked {}",
                status.state, status.pll_locked
            ),
        );
    }
    for r in &b.receivers {
        attempt!(b, "resetting a DMA", r.i.reset());
        attempt!(b, "resetting a DMA", r.q.reset());
    }
    for t in &mut b.transmitters {
        attempt!(b, "silencing a transmitter", t.amplitude.disable());
    }

    // The tone's frequency is the transmit mixer's; the receive mixer takes `f_rx` off it again.
    const TRANSMIT_HZ: [i64; 2] = [300_000_000, 450_000_000];
    const BASEBAND_HZ: [i64; 4] = [30_000_000, -45_000_000, 101_250_000, -7_500_000];
    for n in 0..2 {
        let (name, tile) = (b.transmitters[n].name, b.transmitters[n].tile);
        let (receiver, rx_tile, rx_block) = (
            b.receivers[n].name,
            b.receivers[n].tile,
            b.receivers[n].block,
        );
        let tx = TRANSMIT_HZ[n];
        (b.print)(format_args!("\n{name} at {tx} Hz into {receiver}:\n"));
        attempt!(
            b,
            "tuning the transmitter",
            b.tune(Converter::Dac, tile, 0, tx)
        );
        attempt!(
            b,
            "setting the amplitude",
            b.transmitters[n].amplitude.set_amplitude(AMPLITUDE)
        );
        attempt!(b, "enabling", b.transmitters[n].amplitude.enable());

        for baseband in BASEBAND_HZ {
            let rx = -(tx - baseband);
            attempt!(
                b,
                "tuning the receiver",
                b.tune(Converter::Adc, rx_tile, rx_block, rx)
            );
            let set = attempt!(
                b,
                "reading the mixer",
                b.rfdc.mixer(Converter::Adc, rx_tile, rx_block)
            );
            sleep(Duration::from_millis(10));
            attempt!(b, "recording", b.record(n));
            // A real signal through a mixer comes out twice: at the difference of the frequencies, and, as strong,
            // at minus their sum.
            let image = wrap(rx - tx);
            let (peak, level) = (b.spectrum.peak_except(image), b.spectrum.level(baseband));
            // With I and Q exchanged, the difference would appear on the other side of zero.
            let mirror = b.spectrum.level(-baseband);
            b.check(
                set.frequency_hz == rx && peak == baseband && level > -40.0 && mirror < level - 30.0,
                format_args!(
                    "receive mixer {rx} Hz: strongest but the image ({image} Hz) at {peak} Hz (expected {baseband}), \
                     {level:.1} dBFS there, {mirror:.1} dBFS at {} Hz",
                    -baseband
                ),
            );
        }

        // The last frequency stays: halve the amplitude, then switch off.
        let baseband = BASEBAND_HZ[BASEBAND_HZ.len() - 1];
        let full = b.spectrum.level(baseband);
        attempt!(
            b,
            "halving",
            b.transmitters[n].amplitude.set_amplitude(AMPLITUDE / 2)
        );
        sleep(Duration::from_millis(10));
        attempt!(b, "recording", b.record(n));
        let half = b.spectrum.level(baseband);
        b.check(
            (full - half - 6.02).abs() < 0.5,
            format_args!(
                "half the amplitude: {half:.1} dBFS, {:.2} dB less (expected 6.02)",
                full - half
            ),
        );
        attempt!(
            b,
            "restoring",
            b.transmitters[n].amplitude.set_amplitude(AMPLITUDE)
        );

        // Who hears this transmitter? Every receiver is tuned the same way.
        for r in 0..b.receivers.len() {
            let (tile, block) = (b.receivers[r].tile, b.receivers[r].block);
            attempt!(
                b,
                "tuning a receiver",
                b.tune(Converter::Adc, tile, block, -(tx - baseband))
            );
        }
        sleep(Duration::from_millis(10));
        let mut heard = [0.0; 4];
        for (r, level) in heard.iter_mut().enumerate() {
            attempt!(
                b,
                format_args!("recording {}", b.receivers[r].name),
                b.record(r)
            );
            *level = b.spectrum.level(baseband);
        }
        for (r, level) in heard.into_iter().enumerate() {
            (b.print)(format_args!(
                "       {}: {level:.1} dBFS\n",
                b.receivers[r].name
            ));
        }
        let own = heard[n];
        let others = (0..4).filter(|&r| r != n).all(|r| heard[r] < own - 30.0);
        b.check(
            others,
            format_args!("only {receiver} hears it (the others at least 30 dB down)"),
        );

        attempt!(b, "disabling", b.transmitters[n].amplitude.disable());
        sleep(Duration::from_millis(10));
        attempt!(b, "recording", b.record(n));
        let off = b.spectrum.level(baseband);
        b.check(
            off < own - 40.0,
            format_args!(
                "switched off: {off:.1} dBFS, {:.1} dB down (at least 40)",
                own - off
            ),
        );
    }
    b.failures
}
