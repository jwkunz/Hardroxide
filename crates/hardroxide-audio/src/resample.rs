//! Linear-interpolation resampling of a frame stream (16.16 fixed point, no allocation).

use hardroxide_ip::audio::Frame;

/// Resamples `source` (at `from_hz`) to `to_hz`.
#[derive(Debug, Clone)]
pub struct Resample<I: Iterator<Item = Frame>> {
    source: I,
    /// Input frames advanced per output frame, 16.16.
    step: u64,
    /// Position between `a` (integer part 0) and `b` (1), 16.16.
    pos: u64,
    a: Option<Frame>,
    b: Option<Frame>,
    primed: bool,
}

impl<I: Iterator<Item = Frame>> Resample<I> {
    pub fn new(source: I, from_hz: u32, to_hz: u32) -> Self {
        Resample {
            source,
            step: (u64::from(from_hz) << 16) / u64::from(to_hz.max(1)),
            pos: 0,
            a: None,
            b: None,
            primed: false,
        }
    }
}

fn lerp(a: i32, b: i32, f: u64) -> i32 {
    (i64::from(a) + (((i64::from(b) - i64::from(a)) * f as i64) >> 16)) as i32
}

impl<I: Iterator<Item = Frame>> Iterator for Resample<I> {
    type Item = Frame;
    fn next(&mut self) -> Option<Frame> {
        if !self.primed {
            self.a = self.source.next();
            self.b = self.source.next();
            self.primed = true;
        }
        let a = self.a?;
        let out = match self.b {
            Some(b) => {
                let f = self.pos & 0xFFFF;
                Frame {
                    left: lerp(a.left, b.left, f),
                    right: lerp(a.right, b.right, f),
                }
            }
            None => a,
        };
        self.pos += self.step;
        while self.pos >= 1 << 16 {
            self.pos -= 1 << 16;
            self.a = self.b;
            self.b = self.source.next();
            if self.a.is_none() {
                break;
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    fn f(v: i32) -> Frame {
        Frame { left: v, right: -v }
    }

    #[test]
    fn same_rate_is_the_identity() {
        let src: Vec<Frame> = (0..10).map(|i| f(i * 100)).collect();
        let out: Vec<Frame> = Resample::new(src.iter().copied(), 48_000, 48_000).collect();
        assert_eq!(out, src);
    }

    #[test]
    fn doubling_the_rate_interpolates_between_samples() {
        let src = [f(0), f(100), f(200)];
        let out: Vec<Frame> = Resample::new(src.iter().copied(), 24_000, 48_000).collect();
        assert_eq!(out[..5], [f(0), f(50), f(100), f(150), f(200)]);
    }

    #[test]
    fn the_output_length_follows_the_ratio() {
        let src: Vec<Frame> = (0..8000).map(f).collect();
        let out = Resample::new(src.into_iter(), 8000, 48_828).count();
        assert!((out as i64 - 48_828).abs() < 8, "{out}");
        let down = Resample::new((0..44_100).map(f), 44_100, 22_050).count();
        assert!((down as i64 - 22_050).abs() < 4, "{down}");
    }

    #[test]
    fn empty_and_single_sources() {
        assert_eq!(Resample::new(core::iter::empty(), 1, 2).count(), 0);
        let one: Vec<Frame> = Resample::new([f(7)].into_iter(), 1, 2).collect();
        assert_eq!(one.first().copied(), Some(f(7)));
    }
}
