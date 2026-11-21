//! Tone synthesis: a table-driven sine oscillator with a linear envelope, and note names.

use hardroxide_ip::audio::{Frame, SAMPLE_RATE_HZ};

/// One period of a sine, 256 points, full scale (+-32767).
static SINE: [i16; 256] = [
    0, 804, 1608, 2410, 3212, 4011, 4808, 5602, 6393, 7179, 7962, 8739, 9512, 10278, 11039, 11793,
    12539, 13279, 14010, 14732, 15446, 16151, 16846, 17530, 18204, 18868, 19519, 20159, 20787,
    21403, 22005, 22594, 23170, 23731, 24279, 24811, 25329, 25832, 26319, 26790, 27245, 27683,
    28105, 28510, 28898, 29268, 29621, 29956, 30273, 30571, 30852, 31113, 31356, 31580, 31785,
    31971, 32137, 32285, 32412, 32521, 32609, 32678, 32728, 32757, 32767, 32757, 32728, 32678,
    32609, 32521, 32412, 32285, 32137, 31971, 31785, 31580, 31356, 31113, 30852, 30571, 30273,
    29956, 29621, 29268, 28898, 28510, 28105, 27683, 27245, 26790, 26319, 25832, 25329, 24811,
    24279, 23731, 23170, 22594, 22005, 21403, 20787, 20159, 19519, 18868, 18204, 17530, 16846,
    16151, 15446, 14732, 14010, 13279, 12539, 11793, 11039, 10278, 9512, 8739, 7962, 7179, 6393,
    5602, 4808, 4011, 3212, 2410, 1608, 804, 0, -804, -1608, -2410, -3212, -4011, -4808, -5602,
    -6393, -7179, -7962, -8739, -9512, -10278, -11039, -11793, -12539, -13279, -14010, -14732,
    -15446, -16151, -16846, -17530, -18204, -18868, -19519, -20159, -20787, -21403, -22005, -22594,
    -23170, -23731, -24279, -24811, -25329, -25832, -26319, -26790, -27245, -27683, -28105, -28510,
    -28898, -29268, -29621, -29956, -30273, -30571, -30852, -31113, -31356, -31580, -31785, -31971,
    -32137, -32285, -32412, -32521, -32609, -32678, -32728, -32757, -32767, -32757, -32728, -32678,
    -32609, -32521, -32412, -32285, -32137, -31971, -31785, -31580, -31356, -31113, -30852, -30571,
    -30273, -29956, -29621, -29268, -28898, -28510, -28105, -27683, -27245, -26790, -26319, -25832,
    -25329, -24811, -24279, -23731, -23170, -22594, -22005, -21403, -20787, -20159, -19519, -18868,
    -18204, -17530, -16846, -16151, -15446, -14732, -14010, -13279, -12539, -11793, -11039, -10278,
    -9512, -8739, -7962, -7179, -6393, -5602, -4808, -4011, -3212, -2410, -1608, -804,
];

/// A sine oscillator: a 32-bit phase accumulator indexing the table with linear interpolation.
#[derive(Debug, Clone, Copy)]
pub struct Oscillator {
    phase: u32,
    step: u32,
}

impl Oscillator {
    /// An oscillator of `freq_hz` for a sample rate of `rate_hz`.
    pub fn new(freq_hz: f32, rate_hz: f32) -> Self {
        Oscillator {
            phase: 0,
            step: ((freq_hz / rate_hz) * 4_294_967_296.0) as u32,
        }
    }

    /// Change the frequency, keeping the phase (no click).
    pub fn set_freq(&mut self, freq_hz: f32, rate_hz: f32) {
        self.step = ((freq_hz / rate_hz) * 4_294_967_296.0) as u32;
    }

    /// The next sample, a 16-bit value.
    pub fn next_sample(&mut self) -> i16 {
        let idx = (self.phase >> 24) as usize;
        let frac = ((self.phase >> 8) & 0xFFFF) as i32;
        let (a, b) = (i32::from(SINE[idx]), i32::from(SINE[(idx + 1) & 255]));
        self.phase = self.phase.wrapping_add(self.step);
        (a + (((b - a) * frac) >> 16)) as i16
    }
}

/// A note: a pitch (MIDI numbering, 69 = A4 = 440 Hz) played for a number of samples, with a linear
/// attack and release of `ramp` samples each so it starts and ends without a click.
#[derive(Debug, Clone, Copy)]
pub struct Note {
    osc: Oscillator,
    left: u32,
    total: u32,
    ramp: u32,
}

/// The frequency of MIDI note `n` in hertz.
pub fn midi_hz(n: u8) -> f32 {
    // 440 * 2^((n - 69) / 12), by repeated multiplication of the twelfth root of two.
    const SEMITONE: f32 = 1.059_463_1;
    let mut f = 440.0;
    let mut k = i32::from(n) - 69;
    while k > 0 {
        f *= SEMITONE;
        k -= 1;
    }
    while k < 0 {
        f /= SEMITONE;
        k += 1;
    }
    f
}

impl Note {
    pub fn new(midi: u8, samples: u32, ramp: u32, rate_hz: f32) -> Self {
        Note {
            osc: Oscillator::new(midi_hz(midi), rate_hz),
            left: samples,
            total: samples,
            ramp: ramp.min(samples / 2).max(1),
        }
    }

    /// A note at the controller's sample rate.
    pub fn at_codec_rate(midi: u8, millis: u32) -> Self {
        let rate = SAMPLE_RATE_HZ as f32;
        Note::new(
            midi,
            (rate * millis as f32 / 1000.0) as u32,
            (rate * 0.005) as u32,
            rate,
        )
    }
}

impl Iterator for Note {
    type Item = Frame;
    fn next(&mut self) -> Option<Frame> {
        if self.left == 0 {
            return None;
        }
        let done = self.total - self.left;
        self.left -= 1;
        let gain = if done < self.ramp {
            done * 256 / self.ramp
        } else if self.left < self.ramp {
            self.left * 256 / self.ramp
        } else {
            256
        };
        let s = (i32::from(self.osc.next_sample()) * gain as i32) >> 8;
        Some(Frame::from_i16(s as i16, s as i16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_a_sine() {
        assert_eq!(SINE[0], 0);
        assert_eq!(SINE[64], 32767);
        assert_eq!(SINE[192], -32767);
        assert!(SINE[128].abs() <= 1);
    }

    #[test]
    fn midi_pitches() {
        assert!((midi_hz(69) - 440.0).abs() < 1e-3);
        assert!((midi_hz(81) - 880.0).abs() < 0.5);
        assert!((midi_hz(60) - 261.63).abs() < 0.2);
    }

    #[test]
    fn an_oscillator_has_the_requested_period() {
        // 1 kHz at 48 kHz: 48 samples per period, so sample 48 is back where sample 0 was.
        let mut o = Oscillator::new(1000.0, 48_000.0);
        let v: std::vec::Vec<i16> = (0..96).map(|_| o.next_sample()).collect();
        assert!(
            v[0].abs() < 2 && (i32::from(v[48]) - i32::from(v[0])).abs() < 300,
            "{} {}",
            v[0],
            v[48]
        );
        assert!(v.iter().any(|&s| s > 32000) && v.iter().any(|&s| s < -32000));
    }

    #[test]
    fn a_note_ramps_in_and_out_and_has_its_length() {
        let n: std::vec::Vec<Frame> = Note::new(69, 4800, 240, 48_000.0).collect();
        assert_eq!(n.len(), 4800);
        assert!(n[0].to_i16().0.abs() < 200);
        assert!(n[4799].to_i16().0.abs() < 2000);
        assert!(n[2400..2500].iter().any(|f| f.to_i16().0.abs() > 30000));
    }
}
