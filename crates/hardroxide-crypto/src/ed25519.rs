//! Ed25519 signatures (RFC 8032): signing, public keys and verification, on the twisted Edwards form of Curve25519 with extended
//! coordinates. Signing runs in constant time (the scalar multiplications swap with masks and never branch on the secret); verification
//! uses public data only.

use crate::field::*;
use crate::sha512::{Sha512, sha512};

const D2: Gf = [
    0xf159, 0x26b2, 0x9b94, 0xebd6, 0xb156, 0x8283, 0x149a, 0x00e0, 0xd130, 0xeef3, 0x80f2, 0x198e,
    0xfce7, 0x56df, 0xd9dc, 0x2406,
];
const D: Gf = [
    0x78a3, 0x1359, 0x4dca, 0x75eb, 0xd8ab, 0x4141, 0x0a4d, 0x0070, 0xe898, 0x7779, 0x4079, 0x8cc7,
    0xfe73, 0x2b6f, 0x6cee, 0x5203,
];
const X: Gf = [
    0xd51a, 0x8f25, 0x2d60, 0xc956, 0xa7b2, 0x9525, 0xc760, 0x692c, 0xdc5c, 0xfdd6, 0xe231, 0xc0a4,
    0x53fe, 0xcd6e, 0x36d3, 0x2169,
];
const Y: Gf = [
    0x6658, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666, 0x6666,
    0x6666, 0x6666, 0x6666, 0x6666,
];
const I: Gf = [
    0xa0b0, 0x4a0e, 0x1b27, 0xc4ee, 0xe478, 0xad2f, 0x1806, 0x2f43, 0xd7a7, 0x3dfb, 0x0099, 0x2b4d,
    0xdf0b, 0x4fc1, 0x2480, 0x2b83,
];

/// The group order L = 2^252 + 27742317777372353535851937790883648493, little-endian.
const L: [i64; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

/// A point in extended coordinates (X, Y, Z, T).
type Point = [Gf; 4];

fn point_add(p: &mut Point, q: &Point) {
    let a = mul(&sub(&p[1], &p[0]), &sub(&q[1], &q[0]));
    let b = mul(&add(&p[0], &p[1]), &add(&q[0], &q[1]));
    let c = mul(&mul(&p[3], &q[3]), &D2);
    let d = mul(&p[2], &q[2]);
    let d = add(&d, &d);
    let e = sub(&b, &a);
    let f = sub(&d, &c);
    let g = add(&d, &c);
    let h = add(&b, &a);
    p[0] = mul(&e, &f);
    p[1] = mul(&h, &g);
    p[2] = mul(&g, &f);
    p[3] = mul(&e, &h);
}

fn cswap(p: &mut Point, q: &mut Point, b: i64) {
    for i in 0..4 {
        sel(&mut p[i], &mut q[i], b);
    }
}

fn pack_point(p: &Point) -> [u8; 32] {
    let zi = inv(&p[2]);
    let tx = mul(&p[0], &zi);
    let ty = mul(&p[1], &zi);
    let mut r = pack(&ty);
    r[31] ^= parity(&tx) << 7;
    r
}

/// `s * q` for a 256-bit little-endian scalar, with a constant-time ladder.
fn scalarmult(q: &mut Point, s: &[u8; 32]) -> Point {
    let mut p: Point = [GF0, GF1, GF1, GF0];
    for i in (0..=255).rev() {
        let b = i64::from((s[i / 8] >> (i & 7)) & 1);
        cswap(&mut p, q, b);
        let pp = p;
        point_add(q, &pp);
        let pc = p;
        point_add(&mut p, &pc);
        cswap(&mut p, q, b);
    }
    p
}

fn scalarbase(s: &[u8; 32]) -> Point {
    let mut q: Point = [X, Y, GF1, mul(&X, &Y)];
    scalarmult(&mut q, s)
}

/// Reduce a 512-bit little-endian value modulo L into `r` (32 bytes).
fn mod_l(r: &mut [u8; 32], x: &mut [i64; 64]) {
    for i in (32..64).rev() {
        let mut carry = 0i64;
        let mut j = i - 32;
        while j < i - 12 {
            x[j] += carry - 16 * x[i] * L[j - (i - 32)];
            carry = (x[j] + 128) >> 8;
            x[j] -= carry << 8;
            j += 1;
        }
        x[j] += carry;
        x[i] = 0;
    }
    let mut carry = 0i64;
    for j in 0..32 {
        x[j] += carry - (x[31] >> 4) * L[j];
        carry = x[j] >> 8;
        x[j] &= 255;
    }
    for j in 0..32 {
        x[j] -= carry * L[j];
    }
    for i in 0..32 {
        x[i + 1] += x[i] >> 8;
        r[i] = (x[i] & 255) as u8;
    }
}

/// A 64-byte hash reduced modulo L.
fn reduce(h: &[u8; 64]) -> [u8; 32] {
    let mut x = [0i64; 64];
    for i in 0..64 {
        x[i] = i64::from(h[i]);
    }
    let mut r = [0u8; 32];
    mod_l(&mut r, &mut x);
    r
}

fn expand(seed: &[u8; 32]) -> [u8; 64] {
    let mut d = sha512(seed);
    d[0] &= 248;
    d[31] &= 127;
    d[31] |= 64;
    d
}

/// The 32-byte public key for a 32-byte private seed.
pub fn ed25519_public(seed: &[u8; 32]) -> [u8; 32] {
    let d = expand(seed);
    let p = scalarbase(d[..32].try_into().unwrap());
    pack_point(&p)
}

/// Sign `message` with the private `seed` (whose public key is `public`; pass the result of [`ed25519_public`] to avoid computing it again).
pub fn ed25519_sign_with_public(seed: &[u8; 32], public: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let d = expand(seed);
    // r = H(prefix || M) mod L; R = r * B.
    let mut h = Sha512::new();
    h.update(&d[32..]);
    h.update(message);
    let r = reduce(&h.finish());
    let big_r = pack_point(&scalarbase(&r));
    // k = H(R || A || M) mod L; S = r + k * a mod L.
    let mut h = Sha512::new();
    h.update(&big_r);
    h.update(public);
    h.update(message);
    let k = reduce(&h.finish());
    let mut x = [0i64; 64];
    for i in 0..32 {
        x[i] = i64::from(r[i]);
    }
    for i in 0..32 {
        for j in 0..32 {
            x[i + j] += i64::from(k[i]) * i64::from(d[j]);
        }
    }
    let mut s = [0u8; 32];
    mod_l(&mut s, &mut x);
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&big_r);
    sig[32..].copy_from_slice(&s);
    sig
}

/// Sign `message` with the private `seed`.
pub fn ed25519_sign(seed: &[u8; 32], message: &[u8]) -> [u8; 64] {
    ed25519_sign_with_public(seed, &ed25519_public(seed), message)
}

/// Decode a point and negate it (as verification needs `-A`); `None` if the encoding is not a point on the curve.
fn unpack_neg(p: &[u8; 32]) -> Option<Point> {
    let mut y = unpack(p);
    let one = GF1;
    let num = {
        let y2 = sq(&y);
        sub(&y2, &one)
    };
    let den = add(&mul(&sq(&y), &D), &one);
    // x = sqrt(num / den) by the p = 5 mod 8 method.
    let den2 = sq(&den);
    let den4 = sq(&den2);
    let den6 = mul(&den4, &den2);
    let t = mul(&mul(&den6, &num), &den); // den^7 * num
    let mut x = mul(&mul(&pow2523(&t), &num), &mul(&sq(&den), &den)); // num * den^3 * (num * den^7)^((p-5)/8)
    let chk = mul(&sq(&x), &den);
    if !eq(&chk, &num) {
        x = mul(&x, &I);
    }
    let chk = mul(&sq(&x), &den);
    if !eq(&chk, &num) {
        return None;
    }
    if parity(&x) == (p[31] >> 7) {
        x = sub(&GF0, &x);
    }
    y[15] &= 0x7fff;
    Some([x, y, GF1, mul(&x, &y)])
}

/// Check `signature` on `message` under `public`. Rejects a signature whose S is not below L, as RFC 8032 requires.
pub fn ed25519_verify(public: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let Some(mut q) = unpack_neg(public) else {
        return false;
    };
    let (big_r, s): (&[u8; 32], &[u8; 32]) = (
        signature[..32].try_into().unwrap(),
        signature[32..].try_into().unwrap(),
    );
    // S must be less than L (little-endian compare from the top).
    let mut less = false;
    for i in (0..32).rev() {
        let (a, b) = (i64::from(s[i]), L[i]);
        if a != b {
            less = a < b;
            break;
        }
    }
    if !less {
        return false;
    }
    let mut h = Sha512::new();
    h.update(big_r);
    h.update(public);
    h.update(message);
    let k = reduce(&h.finish());
    // R' = S*B + k*(-A), compared with R.
    let mut p = scalarmult(&mut q, &k);
    let sb = scalarbase(s);
    point_add(&mut p, &sb);
    crate::hmac::constant_time_eq(&pack_point(&p), big_r)
}
