//! ChaCha20-Poly1305 authenticated encryption (RFC 8439 section 2.8), the cipher of TLS 1.3's `TLS_CHACHA20_POLY1305_SHA256`.

use crate::chacha20::{chacha20_block, chacha20_xor};
use crate::hmac::constant_time_eq;
use crate::poly1305::Poly1305;

/// The tag length in bytes.
pub const TAG_LEN: usize = 16;

fn tag(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ciphertext: &[u8]) -> [u8; 16] {
    // The one-time Poly1305 key is the first half of keystream block 0.
    let block0 = chacha20_block(key, 0, nonce);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&block0[..32]);
    let mut p = Poly1305::new(&otk);
    let zeros = [0u8; 16];
    p.update(aad);
    p.update(&zeros[..(16 - aad.len() % 16) % 16]);
    p.update(ciphertext);
    p.update(&zeros[..(16 - ciphertext.len() % 16) % 16]);
    p.update(&(aad.len() as u64).to_le_bytes());
    p.update(&(ciphertext.len() as u64).to_le_bytes());
    p.finish()
}

/// Encrypt `data` in place and return the tag over `aad` and the ciphertext. A (key, nonce) pair must never be used twice.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8]) -> [u8; 16] {
    chacha20_xor(key, 1, nonce, data);
    tag(key, nonce, aad, data)
}

/// Check the tag and, if it is right, decrypt `data` in place and return `true`. On a wrong tag `data` is left as it was (still
/// ciphertext) and the result is `false`.
pub fn open(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    data: &mut [u8],
    tag_in: &[u8; 16],
) -> bool {
    let expected = tag(key, nonce, aad, data);
    if !constant_time_eq(&expected, tag_in) {
        return false;
    }
    chacha20_xor(key, 1, nonce, data);
    true
}
