//! Arithmetic modulo 2^255 - 19 on sixteen 16-bit limbs held in `i64` (the representation of TweetNaCl): no secret-dependent
//! branches or indices. Used by X25519 and Ed25519.

pub(crate) type Gf = [i64; 16];

pub(crate) const GF0: Gf = [0; 16];
pub(crate) const GF1: Gf = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn car(o: &mut Gf) {
    for i in 0..16 {
        o[i] += 1 << 16;
        let c = o[i] >> 16;
        // The carry out of the top limb wraps around multiplied by 38 (2^256 = 38 mod p).
        if i < 15 {
            o[i + 1] += c - 1;
        } else {
            o[0] += 38 * (c - 1);
        }
        o[i] -= c << 16;
    }
}

/// Swap `p` and `q` if `b` is 1 (and not if 0), in constant time.
pub(crate) fn sel(p: &mut Gf, q: &mut Gf, b: i64) {
    let c = !(b - 1);
    for i in 0..16 {
        let t = c & (p[i] ^ q[i]);
        p[i] ^= t;
        q[i] ^= t;
    }
}

/// The canonical 32-byte little-endian form.
pub(crate) fn pack(n: &Gf) -> [u8; 32] {
    let mut t = *n;
    car(&mut t);
    car(&mut t);
    car(&mut t);
    for _ in 0..2 {
        let mut m = GF0;
        m[0] = t[0] - 0xffed;
        for i in 1..15 {
            m[i] = t[i] - 0xffff - ((m[i - 1] >> 16) & 1);
            m[i - 1] &= 0xffff;
        }
        m[15] = t[15] - 0x7fff - ((m[14] >> 16) & 1);
        let b = (m[15] >> 16) & 1;
        m[14] &= 0xffff;
        sel(&mut t, &mut m, 1 - b);
    }
    let mut o = [0u8; 32];
    for i in 0..16 {
        o[2 * i] = (t[i] & 0xff) as u8;
        o[2 * i + 1] = (t[i] >> 8) as u8;
    }
    o
}

pub(crate) fn unpack(n: &[u8; 32]) -> Gf {
    let mut o = GF0;
    for i in 0..16 {
        o[i] = i64::from(n[2 * i]) + (i64::from(n[2 * i + 1]) << 8);
    }
    o[15] &= 0x7fff;
    o
}

pub(crate) fn add(a: &Gf, b: &Gf) -> Gf {
    let mut o = GF0;
    for i in 0..16 {
        o[i] = a[i] + b[i];
    }
    o
}

pub(crate) fn sub(a: &Gf, b: &Gf) -> Gf {
    let mut o = GF0;
    for i in 0..16 {
        o[i] = a[i] - b[i];
    }
    o
}

pub(crate) fn mul(a: &Gf, b: &Gf) -> Gf {
    let mut t = [0i64; 31];
    for i in 0..16 {
        for j in 0..16 {
            t[i + j] += a[i] * b[j];
        }
    }
    for i in 0..15 {
        t[i] += 38 * t[i + 16];
    }
    let mut o = GF0;
    o.copy_from_slice(&t[..16]);
    car(&mut o);
    car(&mut o);
    o
}

pub(crate) fn sq(a: &Gf) -> Gf {
    mul(a, a)
}

/// The inverse, a^(p-2).
pub(crate) fn inv(i: &Gf) -> Gf {
    let mut c = *i;
    for a in (0..=253).rev() {
        c = sq(&c);
        if a != 2 && a != 4 {
            c = mul(&c, i);
        }
    }
    c
}

/// a^((p-5)/8) = a^(2^252 - 3), for square roots.
pub(crate) fn pow2523(i: &Gf) -> Gf {
    let mut c = *i;
    for a in (0..=250).rev() {
        c = sq(&c);
        if a != 1 {
            c = mul(&c, i);
        }
    }
    c
}

/// The parity (lowest bit) of the canonical form.
pub(crate) fn parity(a: &Gf) -> u8 {
    pack(a)[0] & 1
}

/// Whether two field elements are equal, in constant time (1 if equal, else 0).
pub(crate) fn eq(a: &Gf, b: &Gf) -> bool {
    let (x, y) = (pack(a), pack(b));
    let mut d = 0u8;
    for i in 0..32 {
        d |= x[i] ^ y[i];
    }
    core::hint::black_box(d) == 0
}
