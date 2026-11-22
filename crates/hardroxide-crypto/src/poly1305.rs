//! The Poly1305 one-time authenticator (RFC 8439), with 26-bit limbs and 64-bit products (the "donna 32" layout): no secret-dependent
//! branches or indices.

/// An incremental Poly1305 computation. The key must never be used for more than one message.
#[derive(Clone)]
pub struct Poly1305 {
    r: [u32; 5],
    h: [u32; 5],
    pad: [u32; 4],
    buf: [u8; 16],
    buf_len: usize,
}

impl Poly1305 {
    pub fn new(key: &[u8; 32]) -> Self {
        let le = |i: usize| u32::from_le_bytes([key[i], key[i + 1], key[i + 2], key[i + 3]]);
        // r, clamped, in five 26-bit limbs.
        let r = [
            le(0) & 0x3ff_ffff,
            (le(3) >> 2) & 0x3ff_ff03,
            (le(6) >> 4) & 0x3ff_c0ff,
            (le(9) >> 6) & 0x3f0_3fff,
            (le(12) >> 8) & 0x00f_ffff,
        ];
        Poly1305 {
            r,
            h: [0; 5],
            pad: [le(16), le(20), le(24), le(28)],
            buf: [0; 16],
            buf_len: 0,
        }
    }

    fn block(&mut self, m: &[u8; 16], hibit: u32) {
        let le = |i: usize| u32::from_le_bytes([m[i], m[i + 1], m[i + 2], m[i + 3]]);
        let [r0, r1, r2, r3, r4] = self.r;
        let (s1, s2, s3, s4) = (r1 * 5, r2 * 5, r3 * 5, r4 * 5);
        let mut h = self.h;
        h[0] += le(0) & 0x3ff_ffff;
        h[1] += (le(3) >> 2) & 0x3ff_ffff;
        h[2] += (le(6) >> 4) & 0x3ff_ffff;
        h[3] += (le(9) >> 6) & 0x3ff_ffff;
        h[4] += (le(12) >> 8) | hibit;
        let (h0, h1, h2, h3, h4) = (
            u64::from(h[0]),
            u64::from(h[1]),
            u64::from(h[2]),
            u64::from(h[3]),
            u64::from(h[4]),
        );
        let m = |a: u64, b: u32| a * u64::from(b);
        let d0 = m(h0, r0) + m(h1, s4) + m(h2, s3) + m(h3, s2) + m(h4, s1);
        let mut d1 = m(h0, r1) + m(h1, r0) + m(h2, s4) + m(h3, s3) + m(h4, s2);
        let mut d2 = m(h0, r2) + m(h1, r1) + m(h2, r0) + m(h3, s4) + m(h4, s3);
        let mut d3 = m(h0, r3) + m(h1, r2) + m(h2, r1) + m(h3, r0) + m(h4, s4);
        let mut d4 = m(h0, r4) + m(h1, r3) + m(h2, r2) + m(h3, r1) + m(h4, r0);
        // Carry.
        let mut c = (d0 >> 26) as u32;
        let mut h0 = (d0 as u32) & 0x3ff_ffff;
        d1 += u64::from(c);
        c = (d1 >> 26) as u32;
        let h1 = (d1 as u32) & 0x3ff_ffff;
        d2 += u64::from(c);
        c = (d2 >> 26) as u32;
        let h2 = (d2 as u32) & 0x3ff_ffff;
        d3 += u64::from(c);
        c = (d3 >> 26) as u32;
        let h3 = (d3 as u32) & 0x3ff_ffff;
        d4 += u64::from(c);
        c = (d4 >> 26) as u32;
        let h4 = (d4 as u32) & 0x3ff_ffff;
        h0 += c * 5;
        c = h0 >> 26;
        h0 &= 0x3ff_ffff;
        self.h = [h0, h1 + c, h2, h3, h4];
    }

    pub fn update(&mut self, mut data: &[u8]) {
        if self.buf_len > 0 {
            let n = data.len().min(16 - self.buf_len);
            self.buf[self.buf_len..self.buf_len + n].copy_from_slice(&data[..n]);
            self.buf_len += n;
            data = &data[n..];
            if self.buf_len == 16 {
                let b = self.buf;
                self.block(&b, 1 << 24);
                self.buf_len = 0;
            }
        }
        while data.len() >= 16 {
            self.block(data[..16].try_into().unwrap(), 1 << 24);
            data = &data[16..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    /// The 16-byte tag.
    pub fn finish(mut self) -> [u8; 16] {
        if self.buf_len > 0 {
            let mut b = [0u8; 16];
            b[..self.buf_len].copy_from_slice(&self.buf[..self.buf_len]);
            b[self.buf_len] = 1;
            self.block(&b, 0);
        }
        // Fully carry h.
        let [mut h0, mut h1, mut h2, mut h3, mut h4] = self.h;
        let mut c = h1 >> 26;
        h1 &= 0x3ff_ffff;
        h2 += c;
        c = h2 >> 26;
        h2 &= 0x3ff_ffff;
        h3 += c;
        c = h3 >> 26;
        h3 &= 0x3ff_ffff;
        h4 += c;
        c = h4 >> 26;
        h4 &= 0x3ff_ffff;
        h0 += c * 5;
        c = h0 >> 26;
        h0 &= 0x3ff_ffff;
        h1 += c;
        // Compute h + -p (that is h - (2^130 - 5)) and select it if h >= p, without a branch.
        let mut g0 = h0.wrapping_add(5);
        c = g0 >> 26;
        g0 &= 0x3ff_ffff;
        let mut g1 = h1.wrapping_add(c);
        c = g1 >> 26;
        g1 &= 0x3ff_ffff;
        let mut g2 = h2.wrapping_add(c);
        c = g2 >> 26;
        g2 &= 0x3ff_ffff;
        let mut g3 = h3.wrapping_add(c);
        c = g3 >> 26;
        g3 &= 0x3ff_ffff;
        let g4 = h4.wrapping_add(c).wrapping_sub(1 << 26);
        let mask = (g4 >> 31).wrapping_sub(1); // all ones if g4 did not go negative, i.e. h >= p
        let nmask = !mask;
        h0 = (h0 & nmask) | (g0 & mask);
        h1 = (h1 & nmask) | (g1 & mask);
        h2 = (h2 & nmask) | (g2 & mask);
        h3 = (h3 & nmask) | (g3 & mask);
        h4 = (h4 & nmask) | (g4 & mask);
        // h as four 32-bit words, plus the pad.
        let w0 = h0 | (h1 << 26);
        let w1 = (h1 >> 6) | (h2 << 20);
        let w2 = (h2 >> 12) | (h3 << 14);
        let w3 = (h3 >> 18) | (h4 << 8);
        let mut f = u64::from(w0) + u64::from(self.pad[0]);
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&(f as u32).to_le_bytes());
        f = u64::from(w1) + u64::from(self.pad[1]) + (f >> 32);
        out[4..8].copy_from_slice(&(f as u32).to_le_bytes());
        f = u64::from(w2) + u64::from(self.pad[2]) + (f >> 32);
        out[8..12].copy_from_slice(&(f as u32).to_le_bytes());
        f = u64::from(w3) + u64::from(self.pad[3]) + (f >> 32);
        out[12..16].copy_from_slice(&(f as u32).to_le_bytes());
        out
    }
}

/// The Poly1305 tag of `data` under a one-time `key`.
pub fn poly1305(key: &[u8; 32], data: &[u8]) -> [u8; 16] {
    let mut p = Poly1305::new(key);
    p.update(data);
    p.finish()
}
