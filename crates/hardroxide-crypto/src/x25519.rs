//! X25519 key agreement (RFC 7748): the Montgomery ladder on Curve25519, in constant time.

use crate::field::*;

const C121665: Gf = [0xDB41, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// The base point, u = 9.
pub const BASEPOINT: [u8; 32] = {
    let mut b = [0u8; 32];
    b[0] = 9;
    b
};

/// Multiply the point with u-coordinate `point` by the scalar `scalar` (clamped as RFC 7748 says).
pub fn x25519(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    let mut z = *scalar;
    z[31] = (z[31] & 127) | 64;
    z[0] &= 248;
    let x = unpack(point);
    let (mut a, mut b, mut c, mut d) = (GF1, x, GF0, GF1);
    for i in (0..=254).rev() {
        let r = i64::from((z[i >> 3] >> (i & 7)) & 1);
        sel(&mut a, &mut b, r);
        sel(&mut c, &mut d, r);
        let e = add(&a, &c);
        a = sub(&a, &c);
        c = add(&b, &d);
        b = sub(&b, &d);
        d = sq(&e);
        let f = sq(&a);
        a = mul(&c, &a);
        c = mul(&b, &e);
        let e = add(&a, &c);
        a = sub(&a, &c);
        b = sq(&a);
        c = sub(&d, &f);
        a = mul(&c, &C121665);
        a = add(&a, &d);
        c = mul(&c, &a);
        a = mul(&d, &f);
        d = mul(&b, &x);
        b = sq(&e);
        sel(&mut a, &mut b, r);
        sel(&mut c, &mut d, r);
    }
    let zi = inv(&c);
    pack(&mul(&a, &zi))
}

/// The public key for a private key: `x25519(private, BASEPOINT)`.
pub fn x25519_public(private: &[u8; 32]) -> [u8; 32] {
    x25519(private, &BASEPOINT)
}

/// Whether a shared secret is the all-zero value a low-order peer point produces (which TLS says to refuse).
pub fn is_all_zero(secret: &[u8; 32]) -> bool {
    let mut d = 0u8;
    for b in secret {
        d |= b;
    }
    core::hint::black_box(d) == 0
}
