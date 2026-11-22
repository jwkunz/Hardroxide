//! ECDSA on NIST P-256 (FIPS 186-4, secp256r1) with deterministic nonces (RFC 6979): public keys, signing and verification.
//!
//! The field and scalar arithmetic is Montgomery multiplication on eight 32-bit limbs with no secret-dependent branches or indices; the curve
//! arithmetic uses the complete addition formulas of Renes, Costello and Batina (2015) for curves with a = -3, which have no special cases (adding a point
//! to itself or to the point at infinity needs no separate code path), and scalar multiplication always does the same sequence of operations.
//! Signing is therefore constant time in the private key and in the nonce. Verification uses public data only.

use crate::hmac::HmacSha256;

/// A 256-bit number as eight little-endian 32-bit limbs.
type U256 = [u32; 8];

const P_LIMBS: U256 = [
    0xffff_ffff,
    0xffff_ffff,
    0xffff_ffff,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0x0000_0001,
    0xffff_ffff,
];
const N_LIMBS: U256 = [
    0xfc63_2551,
    0xf3b9_cac2,
    0xa717_9e84,
    0xbce6_faad,
    0xffff_ffff,
    0xffff_ffff,
    0x0000_0000,
    0xffff_ffff,
];
const B_LIMBS: U256 = [
    0x27d2_604b,
    0x3bce_3c3e,
    0xcc53_b0f6,
    0x651d_06b0,
    0x7698_86bc,
    0xb3eb_bd55,
    0xaa3a_93e7,
    0x5ac6_35d8,
];
const GX_LIMBS: U256 = [
    0xd898_c296,
    0xf4a1_3945,
    0x2deb_33a0,
    0x7703_7d81,
    0x63a4_40f2,
    0xf8bc_e6e5,
    0xe12c_4247,
    0x6b17_d1f2,
];
const GY_LIMBS: U256 = [
    0x37bf_51f5,
    0xcbb6_4068,
    0x6b31_5ece,
    0x2bce_3357,
    0x7c0f_9e16,
    0x8ee7_eb4a,
    0xfe1a_7f9b,
    0x4fe3_42e2,
];

/// `a - b` with the borrow out (1 if `a < b`).
const fn sbb(a: &U256, b: &U256) -> (U256, u32) {
    let mut r = [0u32; 8];
    let mut borrow = 0u64;
    let mut i = 0;
    while i < 8 {
        let d = (a[i] as u64).wrapping_sub(b[i] as u64).wrapping_sub(borrow);
        r[i] = d as u32;
        borrow = (d >> 63) & 1;
        i += 1;
    }
    (r, borrow as u32)
}

/// `a + b` with the carry out.
const fn adc(a: &U256, b: &U256) -> (U256, u32) {
    let mut r = [0u32; 8];
    let mut carry = 0u64;
    let mut i = 0;
    while i < 8 {
        let s = a[i] as u64 + b[i] as u64 + carry;
        r[i] = s as u32;
        carry = s >> 32;
        i += 1;
    }
    (r, carry as u32)
}

/// `if choose { a } else { b }` without a branch.
const fn select(choose: u32, a: &U256, b: &U256) -> U256 {
    let mask = 0u32.wrapping_sub(choose & 1);
    let mut r = [0u32; 8];
    let mut i = 0;
    while i < 8 {
        r[i] = (a[i] & mask) | (b[i] & !mask);
        i += 1;
    }
    r
}

/// A prime modulus with its Montgomery constants (R = 2^256).
struct Modulus {
    m: U256,
    n0: u32,
    one: U256, // R mod m: the Montgomery form of 1
    r2: U256,  // R^2 mod m: to convert into Montgomery form
}

const fn double_mod(a: &U256, m: &U256) -> U256 {
    let (d, carry) = adc(a, a);
    let (s, borrow) = sbb(&d, m);
    // Subtract m if 2a >= m (the sum overflowed 256 bits, or it is not less than m).
    select(carry | (1 - borrow), &s, &d)
}

impl Modulus {
    const fn new(m: U256) -> Modulus {
        // n0 = -m^-1 mod 2^32 by Newton's iteration (m is odd).
        let mut inv: u32 = 1;
        let mut i = 0;
        while i < 5 {
            inv = inv.wrapping_mul(2u32.wrapping_sub(m[0].wrapping_mul(inv)));
            i += 1;
        }
        let n0 = inv.wrapping_neg();
        // R mod m: double 1 256 times; R^2 mod m: 256 more.
        let mut x = [0u32; 8];
        x[0] = 1;
        let mut k = 0;
        while k < 256 {
            x = double_mod(&x, &m);
            k += 1;
        }
        let one = x;
        k = 0;
        while k < 256 {
            x = double_mod(&x, &m);
            k += 1;
        }
        Modulus { m, n0, one, r2: x }
    }

    /// Montgomery multiplication: a * b / R mod m.
    const fn mul(&self, a: &U256, b: &U256) -> U256 {
        let mut t = [0u32; 10];
        let mut i = 0;
        while i < 8 {
            let mut c = 0u64;
            let mut j = 0;
            while j < 8 {
                let v = t[j] as u64 + a[j] as u64 * b[i] as u64 + c;
                t[j] = v as u32;
                c = v >> 32;
                j += 1;
            }
            let v = t[8] as u64 + c;
            t[8] = v as u32;
            t[9] = (v >> 32) as u32;
            let q = t[0].wrapping_mul(self.n0);
            let v = t[0] as u64 + q as u64 * self.m[0] as u64;
            let mut c = v >> 32;
            j = 1;
            while j < 8 {
                let v = t[j] as u64 + q as u64 * self.m[j] as u64 + c;
                t[j - 1] = v as u32;
                c = v >> 32;
                j += 1;
            }
            let v = t[8] as u64 + c;
            t[7] = v as u32;
            t[8] = t[9] + (v >> 32) as u32;
            i += 1;
        }
        let r = [t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7]];
        let (s, borrow) = sbb(&r, &self.m);
        select(t[8] | (1 - borrow), &s, &r)
    }

    const fn add(&self, a: &U256, b: &U256) -> U256 {
        let (d, carry) = adc(a, b);
        let (s, borrow) = sbb(&d, &self.m);
        select(carry | (1 - borrow), &s, &d)
    }

    const fn sub(&self, a: &U256, b: &U256) -> U256 {
        let (d, borrow) = sbb(a, b);
        let (s, _) = adc(&d, &self.m);
        select(borrow, &s, &d)
    }

    /// Into Montgomery form.
    const fn to_mont(&self, a: &U256) -> U256 {
        self.mul(a, &self.r2)
    }

    /// Out of Montgomery form.
    const fn from_mont(&self, a: &U256) -> U256 {
        let one = [1, 0, 0, 0, 0, 0, 0, 0];
        self.mul(a, &one)
    }

    /// a^(m-2): the inverse (of a value in Montgomery form, giving the inverse in Montgomery form). The exponent is public, so
    /// the branching on its bits leaks nothing.
    fn inv(&self, a: &U256) -> U256 {
        let two = [2, 0, 0, 0, 0, 0, 0, 0];
        let (e, _) = sbb(&self.m, &two);
        let mut r = self.one;
        for limb in (0..8).rev() {
            for bit in (0..32).rev() {
                r = self.mul(&r, &r);
                if (e[limb] >> bit) & 1 == 1 {
                    r = self.mul(&r, a);
                }
            }
        }
        r
    }
}

static FP: Modulus = Modulus::new(P_LIMBS);
static FN: Modulus = Modulus::new(N_LIMBS);

fn from_be(b: &[u8; 32]) -> U256 {
    let mut r = [0u32; 8];
    for i in 0..8 {
        r[7 - i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
    }
    r
}

fn to_be(a: &U256) -> [u8; 32] {
    let mut b = [0u8; 32];
    for i in 0..8 {
        b[4 * i..4 * i + 4].copy_from_slice(&a[7 - i].to_be_bytes());
    }
    b
}

fn is_zero(a: &U256) -> bool {
    let mut x = 0u32;
    for l in a {
        x |= l;
    }
    core::hint::black_box(x) == 0
}

/// 1 if `a < b`.
fn lt(a: &U256, b: &U256) -> u32 {
    sbb(a, b).1
}

/// A point in homogeneous projective coordinates (X : Y : Z), all in Montgomery form modulo p. The point at infinity is (0 : 1 : 0).
#[derive(Clone, Copy)]
struct Point {
    x: U256,
    y: U256,
    z: U256,
}

impl Point {
    fn infinity() -> Point {
        Point {
            x: [0; 8],
            y: FP.one,
            z: [0; 8],
        }
    }

    fn generator() -> Point {
        Point {
            x: FP.to_mont(&GX_LIMBS),
            y: FP.to_mont(&GY_LIMBS),
            z: FP.one,
        }
    }

    fn select(choose: u32, a: &Point, b: &Point) -> Point {
        Point {
            x: select(choose, &a.x, &b.x),
            y: select(choose, &a.y, &b.y),
            z: select(choose, &a.z, &b.z),
        }
    }

    /// Complete addition (Renes-Costello-Batina, algorithm 4, for a = -3): correct for every pair of points, including P + P, P + (-P) and the point at infinity.
    fn add(&self, q: &Point) -> Point {
        let (f, b) = (&FP, FB_MONT);
        let (x1, y1, z1, x2, y2, z2) = (&self.x, &self.y, &self.z, &q.x, &q.y, &q.z);
        let mut t0 = f.mul(x1, x2);
        let mut t1 = f.mul(y1, y2);
        let mut t2 = f.mul(z1, z2);
        let mut t3 = f.add(x1, y1);
        let mut t4 = f.add(x2, y2);
        t3 = f.mul(&t3, &t4);
        t4 = f.add(&t0, &t1);
        t3 = f.sub(&t3, &t4);
        t4 = f.add(y1, z1);
        let mut x3 = f.add(y2, z2);
        t4 = f.mul(&t4, &x3);
        x3 = f.add(&t1, &t2);
        t4 = f.sub(&t4, &x3);
        x3 = f.add(x1, z1);
        let mut y3 = f.add(x2, z2);
        x3 = f.mul(&x3, &y3);
        y3 = f.add(&t0, &t2);
        y3 = f.sub(&x3, &y3);
        let mut z3 = f.mul(&b, &t2);
        x3 = f.sub(&y3, &z3);
        z3 = f.add(&x3, &x3);
        x3 = f.add(&x3, &z3);
        z3 = f.sub(&t1, &x3);
        x3 = f.add(&t1, &x3);
        y3 = f.mul(&b, &y3);
        t1 = f.add(&t2, &t2);
        t2 = f.add(&t1, &t2);
        y3 = f.sub(&y3, &t2);
        y3 = f.sub(&y3, &t0);
        t1 = f.add(&y3, &y3);
        y3 = f.add(&t1, &y3);
        t1 = f.add(&t0, &t0);
        t0 = f.add(&t1, &t0);
        t0 = f.sub(&t0, &t2);
        t1 = f.mul(&t4, &y3);
        t2 = f.mul(&t0, &y3);
        y3 = f.mul(&x3, &z3);
        y3 = f.add(&y3, &t2);
        x3 = f.mul(&t3, &x3);
        x3 = f.sub(&x3, &t1);
        z3 = f.mul(&t4, &z3);
        t1 = f.mul(&t3, &t0);
        z3 = f.add(&z3, &t1);
        Point {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    /// `k * self` for a 256-bit scalar `k` (big-endian bits), doing the same operations whatever the bits are.
    fn mul(&self, k: &U256) -> Point {
        let mut r = Point::infinity();
        for limb in (0..8).rev() {
            for bit in (0..32).rev() {
                r = r.add(&r);
                let t = r.add(self);
                r = Point::select((k[limb] >> bit) & 1, &t, &r);
            }
        }
        r
    }

    /// The affine coordinates (x, y) out of Montgomery form; `None` for the point at infinity.
    fn to_affine(&self) -> Option<(U256, U256)> {
        if is_zero(&self.z) {
            return None;
        }
        let zi = FP.inv(&self.z);
        Some((
            FP.from_mont(&FP.mul(&self.x, &zi)),
            FP.from_mont(&FP.mul(&self.y, &zi)),
        ))
    }
}

/// b in Montgomery form, computed at compile time.
const FB_MONT: U256 = FP_CONST.to_mont(&B_LIMBS);
const FP_CONST: Modulus = Modulus::new(P_LIMBS);

/// Whether `private` is a valid private key: 1 <= d < n.
pub fn p256_private_is_valid(private: &[u8; 32]) -> bool {
    let d = from_be(private);
    !is_zero(&d) && lt(&d, &N_LIMBS) == 1
}

/// The public key for `private` as an uncompressed SEC1 point (0x04 || x || y), 65 bytes. `None` if `private` is not in 1..n.
pub fn p256_public(private: &[u8; 32]) -> Option<[u8; 65]> {
    if !p256_private_is_valid(private) {
        return None;
    }
    let (x, y) = Point::generator().mul(&from_be(private)).to_affine()?;
    let mut out = [0u8; 65];
    out[0] = 4;
    out[1..33].copy_from_slice(&to_be(&x));
    out[33..].copy_from_slice(&to_be(&y));
    Some(out)
}

/// z = the hash as a number, reduced modulo n (a 256-bit hash is below 2n, so one subtraction does it).
fn hash_to_scalar(hash: &[u8; 32]) -> U256 {
    let z = from_be(hash);
    let (s, borrow) = sbb(&z, &N_LIMBS);
    select(borrow, &z, &s)
}

/// The nonce k of RFC 6979 section 3.2 for HMAC-SHA-256 and the group P-256, for private key `x` and hash `h1`.
fn rfc6979_nonce(x: &[u8; 32], h1: &[u8; 32]) -> U256 {
    let h_octets = to_be(&hash_to_scalar(h1)); // bits2octets(h1)
    let mut v = [1u8; 32];
    let mut k = [0u8; 32];
    for sep in [0u8, 1u8] {
        let mut h = HmacSha256::new(&k);
        h.update(&v);
        h.update(&[sep]);
        h.update(x);
        h.update(&h_octets);
        k = h.finish();
        let mut h = HmacSha256::new(&k);
        h.update(&v);
        v = h.finish();
    }
    loop {
        let mut h = HmacSha256::new(&k);
        h.update(&v);
        v = h.finish();
        let cand = from_be(&v);
        if !is_zero(&cand) && lt(&cand, &N_LIMBS) == 1 {
            return cand;
        }
        let mut h = HmacSha256::new(&k);
        h.update(&v);
        h.update(&[0]);
        k = h.finish();
        let mut h = HmacSha256::new(&k);
        h.update(&v);
        v = h.finish();
    }
}

/// Sign the SHA-256 digest `hash` with `private`: returns the raw signature `r || s` (64 bytes), or `None` if `private` is not a valid key.
/// The nonce is deterministic (RFC 6979), so signing needs no random numbers.
pub fn ecdsa_p256_sign(private: &[u8; 32], hash: &[u8; 32]) -> Option<[u8; 64]> {
    if !p256_private_is_valid(private) {
        return None;
    }
    let d = from_be(private);
    let z = hash_to_scalar(hash);
    let k = rfc6979_nonce(private, hash);
    let (x1, _) = Point::generator().mul(&k).to_affine()?;
    // r = x1 mod n
    let (xs, borrow) = sbb(&x1, &N_LIMBS);
    let r = select(borrow, &x1, &xs);
    if is_zero(&r) {
        return None; // astronomically unlikely
    }
    // s = k^-1 (z + r d) mod n, in Montgomery form modulo n.
    let (n, rm, dm, zm, km) = (
        &FN,
        FN.to_mont(&r),
        FN.to_mont(&d),
        FN.to_mont(&z),
        FN.to_mont(&k),
    );
    let t = n.add(&zm, &n.mul(&rm, &dm));
    let s = n.from_mont(&n.mul(&n.inv(&km), &t));
    if is_zero(&s) {
        return None;
    }
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&to_be(&r));
    sig[32..].copy_from_slice(&to_be(&s));
    Some(sig)
}

/// The signature in DER (`SEQUENCE { INTEGER r, INTEGER s }`), as X.509 and TLS carry ECDSA signatures: returns the bytes and their count (at most 72).
pub fn sig_to_der(sig: &[u8; 64]) -> ([u8; 72], usize) {
    let mut out = [0u8; 72];
    let mut n = 2;
    for half in [&sig[..32], &sig[32..]] {
        let mut v = half;
        while v.len() > 1 && v[0] == 0 {
            v = &v[1..];
        }
        let pad = usize::from(v[0] & 0x80 != 0);
        out[n] = 0x02;
        out[n + 1] = (v.len() + pad) as u8;
        n += 2;
        if pad == 1 {
            out[n] = 0;
            n += 1;
        }
        out[n..n + v.len()].copy_from_slice(v);
        n += v.len();
    }
    out[0] = 0x30;
    out[1] = (n - 2) as u8;
    (out, n)
}

/// Check the raw signature `r || s` on `hash` under the uncompressed public key `public`. Variable time (everything is public).
pub fn ecdsa_p256_verify(public: &[u8; 65], hash: &[u8; 32], sig: &[u8; 64]) -> bool {
    if public[0] != 4 {
        return false;
    }
    let (qx, qy) = (
        from_be(public[1..33].try_into().unwrap()),
        from_be(public[33..].try_into().unwrap()),
    );
    if lt(&qx, &P_LIMBS) == 0 || lt(&qy, &P_LIMBS) == 0 {
        return false;
    }
    // The point must be on the curve: y^2 = x^3 - 3x + b.
    let (xm, ym) = (FP.to_mont(&qx), FP.to_mont(&qy));
    let x2 = FP.mul(&xm, &xm);
    let x3 = FP.mul(&x2, &xm);
    let three_x = FP.add(&FP.add(&xm, &xm), &xm);
    let rhs = FP.add(&FP.sub(&x3, &three_x), &FB_MONT);
    if FP.mul(&ym, &ym) != rhs {
        return false;
    }
    let (r, s) = (
        from_be(sig[..32].try_into().unwrap()),
        from_be(sig[32..].try_into().unwrap()),
    );
    if is_zero(&r) || is_zero(&s) || lt(&r, &N_LIMBS) == 0 || lt(&s, &N_LIMBS) == 0 {
        return false;
    }
    let z = hash_to_scalar(hash);
    let n = &FN;
    let w = n.inv(&n.to_mont(&s));
    let u1 = n.from_mont(&n.mul(&n.to_mont(&z), &w));
    let u2 = n.from_mont(&n.mul(&n.to_mont(&r), &w));
    let q = Point {
        x: xm,
        y: ym,
        z: FP.one,
    };
    let sum = Point::generator().mul(&u1).add(&q.mul(&u2));
    let Some((x1, _)) = sum.to_affine() else {
        return false;
    };
    let (xs, borrow) = sbb(&x1, &N_LIMBS);
    let v = select(borrow, &x1, &xs);
    v == r
}
