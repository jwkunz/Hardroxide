//! Audio for the PYNQ base overlay.
//!
//! * [`adau1761`]: the codec chip, configured over I2C (works over any `embedded-hal` I2C bus).
//! * [`wav`]: parse WAV files in memory, write a header.
//! * [`tone`]: a sine oscillator, notes with a click-free envelope, MIDI pitches.
//! * [`resample`]: linear resampling of a frame stream.
//! * [`io`]: moving frames through the overlay's I2S controller ([`hardroxide_ip::audio`]):
//!   playback, recording, pass-through, and mixing helpers.
//!
//! `no_std`, no allocation. The register sequences follow PYNQ's audio driver and the controller's
//! VHDL source (Apache-2.0); see the reference page for what has and has not been run on a board.

#![cfg_attr(not(any(feature = "std", test)), no_std)]

pub mod adau1761;
pub mod io;
pub mod resample;
pub mod tone;
pub mod wav;

pub use hardroxide_ip::audio::{AudioCodecCtrl, Frame, SAMPLE_RATE_HZ};
