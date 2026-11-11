//! Xilinx `.bit` container parsing and conversion to the raw `.bin` payload.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BitError {
    #[error("truncated .bit header")]
    Truncated,
    #[error("unexpected .bit header field tag {0:#04x}")]
    BadTag(u8),
    #[error("bitstream length {0} is not a multiple of 4")]
    Unaligned(usize),
}

/// A parsed `.bit` file. The header fields carry build timestamps, which is
/// why only [`BitFile::payload`] is embedded in generated crates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitFile<'a> {
    pub design_name: String,
    pub part: String,
    pub date: String,
    pub time: String,
    /// Raw configuration stream (starts with the sync-word preamble), in the
    /// big-endian word order used by the `.bit` file.
    pub payload: &'a [u8],
}

impl<'a> BitFile<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, BitError> {
        let mut p = 0usize;
        let mut take = |n: usize| -> Result<&'a [u8], BitError> {
            let s = data.get(p..p + n).ok_or(BitError::Truncated)?;
            p += n;
            Ok(s)
        };
        let l = u16::from_be_bytes(take(2)?.try_into().unwrap()) as usize;
        take(l)?; // magic field
        take(2)?; // length-of-1 marker
        let mut strings = Vec::new();
        for want in [b'a', b'b', b'c', b'd'] {
            let tag = take(1)?[0];
            if tag != want {
                return Err(BitError::BadTag(tag));
            }
            let n = u16::from_be_bytes(take(2)?.try_into().unwrap()) as usize;
            let s = take(n)?;
            let s = s.split(|&b| b == 0).next().unwrap_or(&[]);
            strings.push(String::from_utf8_lossy(s).into_owned());
        }
        let tag = take(1)?[0];
        if tag != b'e' {
            return Err(BitError::BadTag(tag));
        }
        let n = u32::from_be_bytes(take(4)?.try_into().unwrap()) as usize;
        let payload = take(n)?;
        let mut it = strings.into_iter();
        Ok(BitFile {
            design_name: it.next().unwrap(),
            part: it.next().unwrap(),
            date: it.next().unwrap(),
            time: it.next().unwrap(),
            payload,
        })
    }

    /// Payload as the `.bin` expected by the Linux `fpga_manager` on
    /// Zynq-7000 (`xlnx,zynq-devcfg`): the configuration words byte-swapped
    /// to little-endian, matching `bootgen -process_bitstream bin`.
    ///
    /// NOTE: not yet validated on hardware (see docs/06-session-handoff.md).
    pub fn to_bin_swapped(&self) -> Result<Vec<u8>, BitError> {
        if self.payload.len() % 4 != 0 {
            return Err(BitError::Unaligned(self.payload.len()));
        }
        Ok(self
            .payload
            .chunks_exact(4)
            .flat_map(|w| [w[3], w[2], w[1], w[0]])
            .collect())
    }
}
