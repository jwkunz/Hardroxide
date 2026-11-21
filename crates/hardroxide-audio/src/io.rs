//! Moving samples through the controller: playback, recording and pass-through, one frame per
//! sample period, by polling. The controller holds a single frame in each direction, so these
//! loops must run every ~20.5 us; on a core with other work to do, run them from an interrupt
//! or a dedicated task (see the RTOS application).

use hardroxide_ip::audio::{AudioCodecCtrl, Frame};
use hardroxide_mmio::{Mmio, MmioError};

/// How a stream went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// Frames moved.
    pub frames: u32,
    /// Times the next sample had already arrived when the loop came back for it: the loop was
    /// too slow and a sample was missed.
    pub missed: u32,
}

/// Spin limit while waiting for a sample: far more than one period, so a dead controller (no
/// clock) ends the loop instead of hanging it.
const SPINS: u32 = 2_000_000;

/// Play `frames` to the codec. Stops early (returning what was done) if the controller stops
/// producing sample ticks.
pub fn play<R: Mmio>(
    ctrl: &AudioCodecCtrl<R>,
    frames: impl Iterator<Item = Frame>,
) -> Result<Stats, MmioError> {
    let mut st = Stats::default();
    ctrl.clear_ready()?;
    for f in frames {
        if ctrl.ready()? && st.frames > 0 {
            st.missed += 1;
        }
        if !ctrl.wait_sample(SPINS)? {
            break;
        }
        ctrl.write_frame(f)?;
        st.frames += 1;
    }
    Ok(st)
}

/// Record `out.len()` frames from the codec.
pub fn record<R: Mmio>(ctrl: &AudioCodecCtrl<R>, out: &mut [Frame]) -> Result<Stats, MmioError> {
    let mut st = Stats::default();
    ctrl.clear_ready()?;
    for slot in out.iter_mut() {
        if ctrl.ready()? && st.frames > 0 {
            st.missed += 1;
        }
        if !ctrl.wait_sample(SPINS)? {
            break;
        }
        *slot = ctrl.read_frame()?;
        st.frames += 1;
    }
    Ok(st)
}

/// Play `frames` while recording what comes back in the same sample loop (the controller is full duplex): each
/// sample tick, the frame received is stored in `out` and the next frame is sent. Stops when `out` is full or
/// `frames` ends. With a cable from the output to the input this records what was played.
pub fn play_record<R: Mmio>(
    ctrl: &AudioCodecCtrl<R>,
    frames: impl Iterator<Item = Frame>,
    out: &mut [Frame],
) -> Result<Stats, MmioError> {
    let mut st = Stats::default();
    ctrl.clear_ready()?;
    for (f, slot) in frames.zip(out.iter_mut()) {
        if ctrl.ready()? && st.frames > 0 {
            st.missed += 1;
        }
        if !ctrl.wait_sample(SPINS)? {
            break;
        }
        *slot = ctrl.read_frame()?;
        ctrl.write_frame(f)?;
        st.frames += 1;
    }
    Ok(st)
}

/// Copy the input to the output for `frames` sample periods, passing each frame through `f`.
pub fn pass_through<R: Mmio>(
    ctrl: &AudioCodecCtrl<R>,
    frames: u32,
    mut f: impl FnMut(Frame) -> Frame,
) -> Result<Stats, MmioError> {
    let mut st = Stats::default();
    ctrl.clear_ready()?;
    for _ in 0..frames {
        if ctrl.ready()? && st.frames > 0 {
            st.missed += 1;
        }
        if !ctrl.wait_sample(SPINS)? {
            break;
        }
        ctrl.write_frame(f(ctrl.read_frame()?))?;
        st.frames += 1;
    }
    Ok(st)
}

/// Add two frames, saturating at 24 bits.
pub fn mix(a: Frame, b: Frame) -> Frame {
    let sat = |v: i32| v.clamp(-(1 << 23), (1 << 23) - 1);
    Frame {
        left: sat(a.left + b.left),
        right: sat(a.right + b.right),
    }
}

/// Scale a frame by `gain / 256`.
pub fn scale(f: Frame, gain: u32) -> Frame {
    let s = |v: i32| ((i64::from(v) * i64::from(gain)) >> 8) as i32;
    Frame {
        left: s(f.left),
        right: s(f.right),
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn mixing_saturates() {
        let big = Frame {
            left: 8_000_000,
            right: -8_000_000,
        };
        let m = mix(big, big);
        assert_eq!(
            m,
            Frame {
                left: (1 << 23) - 1,
                right: -(1 << 23)
            }
        );
        assert_eq!(
            mix(Frame { left: 1, right: 2 }, Frame { left: 3, right: 4 }),
            Frame { left: 4, right: 6 }
        );
    }

    #[test]
    fn scaling_by_gain() {
        let f = Frame {
            left: 1000,
            right: -1000,
        };
        assert_eq!(scale(f, 256), f);
        assert_eq!(
            scale(f, 128),
            Frame {
                left: 500,
                right: -500
            }
        );
        assert_eq!(scale(f, 0), Frame::default());
    }

    #[test]
    fn a_stopped_controller_ends_playback_instead_of_hanging() {
        // The ready flag clears on a write of 1 and nothing sets it again, as with a dead I2S clock.
        let be = MockBackend::new();
        be.set_write1_clear(0x43C0_0010);
        let a = AudioCodecCtrl::new(be.map(0x43C0_0000, 0x1000).unwrap());
        let st = play(&a, [Frame::default(); 4].into_iter()).unwrap();
        assert_eq!(st.frames, 0);
        let mut buf = [Frame::default(); 2];
        assert_eq!(record(&a, &mut buf).unwrap().frames, 0);
        assert_eq!(pass_through(&a, 2, |f| f).unwrap().frames, 0);
        let mut rx = [Frame::default(); 2];
        assert_eq!(
            play_record(&a, [Frame::default(); 2].into_iter(), &mut rx)
                .unwrap()
                .frames,
            0
        );
    }
}
