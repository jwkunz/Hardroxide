//! Cryptographic primitives written for Hardroxide's TLS 1.3 server: SHA-256, SHA-512, HMAC-SHA-256, HKDF, ChaCha20-Poly1305, X25519 and Ed25519.
//!
//! `no_std`, no allocation, no dependencies. **Home-made and not audited**: for education and a trusted network. Every primitive is tested against the
//! standard vectors and, on random inputs, against OpenSSL and Python's `cryptography` (see `tests/`).

#![no_std]

pub mod aead;
pub mod chacha20;
pub mod ed25519;
mod field;
pub mod hmac;
pub mod poly1305;
pub mod sha256;
pub mod sha512;
pub mod x25519;

pub use aead::{TAG_LEN, open, seal};
pub use ed25519::{ed25519_public, ed25519_sign, ed25519_sign_with_public, ed25519_verify};
pub use hmac::{HmacSha256, constant_time_eq, hkdf_expand, hkdf_extract, hmac_sha256};
pub use sha256::{SHA256_LEN, Sha256, sha256};
pub use sha512::{SHA512_LEN, Sha512, sha512};
pub use x25519::{BASEPOINT, x25519, x25519_public};
