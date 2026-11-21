//! Windows bitmap (BMP) files: read an uncompressed 24- or 32-bit image into a pixel buffer, and write a
//! surface as a 24-bit BMP, row by row so no second copy of the image is needed.

/// Why a file could not be read as a BMP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BmpError {
    #[error("not a BMP file")]
    NotBmp,
    #[error("the file is cut short")]
    Truncated,
    #[error("only uncompressed 24 and 32 bits per pixel are supported")]
    Unsupported,
}

/// The header of a BMP: its size and where the pixels start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BmpInfo {
    pub width: u32,
    pub height: u32,
    pub bits_per_pixel: u16,
    /// Rows are stored bottom to top (the usual case).
    pub bottom_up: bool,
    /// Offset of the first pixel byte in the file.
    pub data_offset: u32,
    /// Bytes per stored row (padded to a multiple of four).
    pub row_bytes: u32,
}

fn u16le(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}
fn u32le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

impl BmpInfo {
    /// Parse the first 54 bytes of a BMP file (a `BITMAPINFOHEADER` image).
    pub fn parse(header: &[u8]) -> Result<BmpInfo, BmpError> {
        if header.len() < 2 || &header[..2] != b"BM" {
            return Err(BmpError::NotBmp);
        }
        let data_offset = u32le(header, 10).ok_or(BmpError::Truncated)?;
        let dib = u32le(header, 14).ok_or(BmpError::Truncated)?;
        if dib < 40 {
            return Err(BmpError::Unsupported);
        }
        let width = u32le(header, 18).ok_or(BmpError::Truncated)? as i32;
        let height = u32le(header, 22).ok_or(BmpError::Truncated)? as i32;
        let bits = u16le(header, 28).ok_or(BmpError::Truncated)?;
        let compression = u32le(header, 30).ok_or(BmpError::Truncated)?;
        if width <= 0 || height == 0 || !matches!(bits, 24 | 32) || !(compression == 0 || compression == 3 && bits == 32) {
            return Err(BmpError::Unsupported);
        }
        Ok(BmpInfo {
            width: width as u32,
            height: height.unsigned_abs(),
            bits_per_pixel: bits,
            bottom_up: height > 0,
            data_offset,
            row_bytes: (width as u32 * u32::from(bits) / 8).next_multiple_of(4),
        })
    }

    /// The stored row that holds image row `y` (counting from the top).
    pub fn stored_row(&self, y: u32) -> u32 {
        if self.bottom_up { self.height - 1 - y } else { y }
    }

    /// File offset of image row `y` (from the top).
    pub fn row_offset(&self, y: u32) -> u64 {
        u64::from(self.data_offset) + u64::from(self.stored_row(y)) * u64::from(self.row_bytes)
    }

    /// Convert `width` pixels of a stored row to `0x00RRGGBB`.
    pub fn convert_row(&self, row: &[u8], out: &mut [u32]) {
        let bytes = usize::from(self.bits_per_pixel / 8);
        for (px, o) in row.chunks_exact(bytes).zip(out.iter_mut()) {
            *o = u32::from(px[2]) << 16 | u32::from(px[1]) << 8 | u32::from(px[0]);
        }
    }
}

/// Parse a whole BMP held in memory into `out` (`width * height` pixels, row by row from the top).
/// Returns the image size; `out` must be large enough.
pub fn decode(file: &[u8], out: &mut [u32]) -> Result<(u32, u32), BmpError> {
    let info = BmpInfo::parse(file)?;
    let (w, h) = (info.width as usize, info.height as usize);
    if out.len() < w * h {
        return Err(BmpError::Unsupported);
    }
    for y in 0..h {
        let start = info.row_offset(y as u32) as usize;
        let row = file.get(start..start + w * usize::from(info.bits_per_pixel / 8)).ok_or(BmpError::Truncated)?;
        info.convert_row(row, &mut out[y * w..(y + 1) * w]);
    }
    Ok((info.width, info.height))
}

/// The 54-byte header of a 24-bit bottom-up BMP of `width` x `height`.
pub fn header_24bit(width: u32, height: u32) -> [u8; 54] {
    let row = (width * 3).next_multiple_of(4);
    let size = 54 + row * height;
    let mut h = [0u8; 54];
    h[..2].copy_from_slice(b"BM");
    h[2..6].copy_from_slice(&size.to_le_bytes());
    h[10..14].copy_from_slice(&54u32.to_le_bytes());
    h[14..18].copy_from_slice(&40u32.to_le_bytes());
    h[18..22].copy_from_slice(&width.to_le_bytes());
    h[22..26].copy_from_slice(&height.to_le_bytes());
    h[26..28].copy_from_slice(&1u16.to_le_bytes());
    h[28..30].copy_from_slice(&24u16.to_le_bytes());
    h[34..38].copy_from_slice(&(row * height).to_le_bytes());
    h[38..42].copy_from_slice(&2835u32.to_le_bytes());
    h[42..46].copy_from_slice(&2835u32.to_le_bytes());
    h
}

/// Encode one row of pixels (`0x00RRGGBB`) as a padded 24-bit BMP row into `out`
/// (`(width * 3).next_multiple_of(4)` bytes).
pub fn encode_row(pixels: &[u32], out: &mut [u8]) {
    for (p, o) in pixels.iter().zip(out.chunks_exact_mut(3)) {
        o[0] = *p as u8;
        o[1] = (*p >> 8) as u8;
        o[2] = (*p >> 16) as u8;
    }
    let used = pixels.len() * 3;
    out[used..].fill(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(w: u32, h: u32, px: &[u32]) -> std::vec::Vec<u8> {
        let mut f = header_24bit(w, h).to_vec();
        let row = (w * 3).next_multiple_of(4) as usize;
        // Bottom-up: the last image row is stored first.
        for y in (0..h as usize).rev() {
            let mut r = std::vec![0u8; row];
            encode_row(&px[y * w as usize..(y + 1) * w as usize], &mut r);
            f.extend(r);
        }
        f
    }

    #[test]
    fn an_image_round_trips_with_row_padding() {
        // 5 pixels wide: 15 bytes of pixels, padded to 16 per row.
        let px: std::vec::Vec<u32> = (0..15).map(|i| 0x0001_0203 * (i + 1)).collect();
        let f = encode(5, 3, &px);
        assert_eq!(f.len(), 54 + 16 * 3);
        let info = BmpInfo::parse(&f).unwrap();
        assert_eq!((info.width, info.height, info.bits_per_pixel, info.row_bytes), (5, 3, 24, 16));
        let mut out = std::vec![0u32; 15];
        assert_eq!(decode(&f, &mut out).unwrap(), (5, 3));
        assert_eq!(out, px);
    }

    #[test]
    fn a_32_bit_top_down_file_is_read() {
        let mut f = header_24bit(2, 2).to_vec();
        f[28] = 32;
        f[22..26].copy_from_slice(&(-2i32).to_le_bytes()); // top-down
        for p in [[1u8, 2, 3, 0], [4, 5, 6, 0], [7, 8, 9, 0], [10, 11, 12, 0]] {
            f.extend(p);
        }
        let mut out = [0u32; 4];
        assert_eq!(decode(&f, &mut out).unwrap(), (2, 2));
        assert_eq!(out, [0x030201, 0x060504, 0x090807, 0x0C0B0A]);
    }

    #[test]
    fn bad_files_are_refused() {
        assert_eq!(BmpInfo::parse(b"PNG...").unwrap_err(), BmpError::NotBmp);
        let mut f = header_24bit(1, 1).to_vec();
        f[28] = 8;
        assert_eq!(BmpInfo::parse(&f).unwrap_err(), BmpError::Unsupported);
        let mut g = header_24bit(2, 2).to_vec();
        g.extend([0u8; 4]);
        assert_eq!(decode(&g, &mut [0u32; 4]).unwrap_err(), BmpError::Truncated);
    }
}
