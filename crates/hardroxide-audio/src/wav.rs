//! WAV (RIFF, PCM) files: parse one in memory and write a header.

use hardroxide_ip::audio::Frame;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WavError {
    #[error("not a RIFF/WAVE file")]
    NotWav,
    #[error("the file is cut short")]
    Truncated,
    #[error("no fmt chunk before the data")]
    NoFormat,
    #[error("no data chunk")]
    NoData,
    #[error("only PCM with 1 or 2 channels and 8, 16 or 24 bits is supported")]
    Unsupported,
}

/// A parsed WAV file borrowing its bytes.
#[derive(Debug, Clone, Copy)]
pub struct Wav<'a> {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: u16,
    data: &'a [u8],
}

fn u16le(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}
fn u32le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(i)?,
        *b.get(i + 1)?,
        *b.get(i + 2)?,
        *b.get(i + 3)?,
    ]))
}

impl<'a> Wav<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Wav<'a>, WavError> {
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(WavError::NotWav);
        }
        let mut fmt: Option<(u32, u16, u16)> = None;
        let mut i = 12;
        while i + 8 <= bytes.len() {
            let id = &bytes[i..i + 4];
            let len = u32le(bytes, i + 4).ok_or(WavError::Truncated)? as usize;
            let body = i + 8;
            if id == b"fmt " {
                let tag = u16le(bytes, body).ok_or(WavError::Truncated)?;
                let channels = u16le(bytes, body + 2).ok_or(WavError::Truncated)?;
                let rate = u32le(bytes, body + 4).ok_or(WavError::Truncated)?;
                let bits = u16le(bytes, body + 14).ok_or(WavError::Truncated)?;
                // 1 = PCM; 0xFFFE = extensible, accepted when it carries PCM (sub-format first word 1).
                let pcm = tag == 1 || (tag == 0xFFFE && u16le(bytes, body + 24) == Some(1));
                if !pcm || !(1..=2).contains(&channels) || !matches!(bits, 8 | 16 | 24) {
                    return Err(WavError::Unsupported);
                }
                fmt = Some((rate, channels, bits));
            } else if id == b"data" {
                let (rate, channels, bits) = fmt.ok_or(WavError::NoFormat)?;
                // A data length past the end (a streamed file) means "to the end".
                let end = body.saturating_add(len).min(bytes.len());
                if body > bytes.len() {
                    return Err(WavError::Truncated);
                }
                return Ok(Wav {
                    sample_rate: rate,
                    channels,
                    bits,
                    data: &bytes[body..end],
                });
            }
            i = body.saturating_add(len).saturating_add(len & 1); // chunks are padded to even sizes
        }
        Err(WavError::NoData)
    }

    /// Bytes per frame (all channels).
    pub fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * usize::from(self.bits / 8)
    }

    /// Number of frames.
    pub fn len(&self) -> usize {
        self.data.len() / self.frame_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Duration in milliseconds.
    pub fn millis(&self) -> u64 {
        self.len() as u64 * 1000 / u64::from(self.sample_rate.max(1))
    }

    /// The frames, converted to the controller's 24-bit stereo (mono is duplicated).
    pub fn frames(&self) -> Frames<'a> {
        Frames { wav: *self, pos: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct Frames<'a> {
    wav: Wav<'a>,
    pos: usize,
}

impl Frames<'_> {
    fn sample(&self, at: usize) -> i32 {
        let d = self.wav.data;
        match self.wav.bits {
            8 => (i32::from(d[at]) - 128) << 16,
            16 => i32::from(i16::from_le_bytes([d[at], d[at + 1]])) << 8,
            _ => i32::from_le_bytes([0, d[at], d[at + 1], d[at + 2]]) >> 8,
        }
    }
}

impl Iterator for Frames<'_> {
    type Item = Frame;
    fn next(&mut self) -> Option<Frame> {
        let fb = self.wav.frame_bytes();
        let at = self.pos * fb;
        if at + fb > self.wav.data.len() {
            return None;
        }
        self.pos += 1;
        let step = fb / usize::from(self.wav.channels);
        let left = self.sample(at);
        let right = if self.wav.channels == 2 {
            self.sample(at + step)
        } else {
            left
        };
        Some(Frame { left, right })
    }
}

/// The 44-byte header of a 16-bit PCM WAV file with `data_len` bytes of samples.
pub fn header_16bit(sample_rate: u32, channels: u16, data_len: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    let block = channels * 2;
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate * u32::from(block)).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    fn file(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let mut v = header_16bit(rate, channels, (samples.len() * 2) as u32).to_vec();
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    #[test]
    fn parses_stereo_16_bit() {
        let f = file(44_100, 2, &[100, -100, 200, -200, 300, -300]);
        let w = Wav::parse(&f).unwrap();
        assert_eq!(
            (w.sample_rate, w.channels, w.bits, w.len()),
            (44_100, 2, 16, 3)
        );
        let frames: Vec<Frame> = w.frames().collect();
        assert_eq!(frames[0].to_i16(), (100, -100));
        assert_eq!(frames[2].to_i16(), (300, -300));
    }

    #[test]
    fn mono_is_duplicated() {
        let f = file(8000, 1, &[1000, -1000]);
        let frames: Vec<Frame> = Wav::parse(&f).unwrap().frames().collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].to_i16(), (-1000, -1000));
        assert_eq!(Wav::parse(&f).unwrap().millis(), 0);
    }

    #[test]
    fn eight_and_twenty_four_bit() {
        // 8-bit is unsigned with 128 as silence.
        let mut f = header_16bit(8000, 1, 2).to_vec();
        f[34] = 8;
        f.extend_from_slice(&[128, 255]);
        let fr: Vec<Frame> = Wav::parse(&f).unwrap().frames().collect();
        assert_eq!(fr[0].left, 0);
        assert_eq!(fr[1].left, 127 << 16);
        // 24-bit little endian: -2 is FE FF FF.
        let mut g = header_16bit(8000, 1, 3).to_vec();
        g[34] = 24;
        g.extend_from_slice(&[0xFE, 0xFF, 0xFF]);
        assert_eq!(Wav::parse(&g).unwrap().frames().next().unwrap().left, -2);
    }

    #[test]
    fn skips_unknown_chunks_and_pads() {
        let mut f = header_16bit(8000, 1, 0).to_vec();
        f.truncate(36);
        f.extend_from_slice(b"LIST\x03\x00\x00\x00abc\x00"); // odd length, padded
        f.extend_from_slice(b"data\x02\x00\x00\x00\x10\x00");
        let w = Wav::parse(&f).unwrap();
        assert_eq!(w.frames().next().unwrap().to_i16(), (16, 16));
    }

    #[test]
    fn rejects_what_it_cannot_play() {
        assert_eq!(Wav::parse(b"nope").unwrap_err(), WavError::NotWav);
        let mut f = file(8000, 1, &[0]);
        f[20] = 3; // IEEE float
        assert_eq!(Wav::parse(&f).unwrap_err(), WavError::Unsupported);
        let mut g = file(8000, 1, &[0]);
        g.truncate(30);
        assert!(Wav::parse(&g).is_err());
        let mut h = file(8000, 1, &[0]);
        h[36..40].copy_from_slice(b"junk");
        assert_eq!(Wav::parse(&h).unwrap_err(), WavError::NoData);
    }

    #[test]
    fn a_data_length_past_the_end_means_to_the_end() {
        let mut f = file(8000, 1, &[1, 2, 3, 4]);
        f[40..44].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(Wav::parse(&f).unwrap().len(), 4);
    }
}
