//! HMAC-SHA-256 (RFC 2104) and HKDF (RFC 5869).

use crate::sha256::{SHA256_BLOCK, SHA256_LEN, Sha256};

/// An incremental HMAC-SHA-256.
#[derive(Clone)]
pub struct HmacSha256 {
    inner: Sha256,
    outer_key: [u8; SHA256_BLOCK],
}

impl HmacSha256 {
    pub fn new(key: &[u8]) -> Self {
        let mut k = [0u8; SHA256_BLOCK];
        if key.len() > SHA256_BLOCK {
            k[..SHA256_LEN].copy_from_slice(&crate::sha256::sha256(key));
        } else {
            k[..key.len()].copy_from_slice(key);
        }
        let mut ipad = [0x36u8; SHA256_BLOCK];
        let mut opad = [0x5cu8; SHA256_BLOCK];
        for i in 0..SHA256_BLOCK {
            ipad[i] ^= k[i];
            opad[i] ^= k[i];
        }
        let mut inner = Sha256::new();
        inner.update(&ipad);
        HmacSha256 {
            inner,
            outer_key: opad,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    pub fn finish(self) -> [u8; 32] {
        let ih = self.inner.finish();
        let mut o = Sha256::new();
        o.update(&self.outer_key);
        o.update(&ih);
        o.finish()
    }
}

/// HMAC-SHA-256 of `data` under `key`.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut h = HmacSha256::new(key);
    h.update(data);
    h.finish()
}

/// HKDF-Extract: the pseudorandom key from a salt and input keying material.
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    // An empty salt is a string of zeros as long as the hash.
    if salt.is_empty() {
        hmac_sha256(&[0u8; SHA256_LEN], ikm)
    } else {
        hmac_sha256(salt, ikm)
    }
}

/// HKDF-Expand: fill `okm` (at most 255 x 32 bytes) from a pseudorandom key and context `info`.
pub fn hkdf_expand(prk: &[u8; 32], info: &[u8], okm: &mut [u8]) {
    assert!(okm.len() <= 255 * SHA256_LEN, "HKDF output too long");
    let mut t = [0u8; SHA256_LEN];
    let mut have = 0usize;
    for (counter, chunk) in okm.chunks_mut(SHA256_LEN).enumerate() {
        let mut h = HmacSha256::new(prk);
        if counter > 0 {
            h.update(&t);
        }
        h.update(info);
        h.update(&[(counter + 1) as u8]);
        t = h.finish();
        chunk.copy_from_slice(&t[..chunk.len()]);
        have += chunk.len();
    }
    debug_assert_eq!(have, okm.len());
}

/// Compare two byte strings in constant time (for MACs and tags). Different lengths compare unequal at once.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    core::hint::black_box(diff) == 0
}
