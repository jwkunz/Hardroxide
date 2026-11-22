# hardroxide-crypto

The primitives of the TLS 1.3 server, written for this project: SHA-256, SHA-512, HMAC-SHA-256, HKDF, ChaCha20-Poly1305, X25519, Ed25519, ECDSA on P-256 and a random bit generator. `no_std`, no allocation,
no dependencies.

> **This is home-made cryptography, written to be understood and for a trusted network. It has not been audited, and nobody but this project has reviewed it.** What was done to make it right is listed
> under "Testing"; what was not done, under "What is not verified". Do not use it where a failure would matter more than an experiment does.

## Hashes, MAC and key derivation

```rust,ignore
pub const SHA256_LEN: usize = 32
pub const SHA256_BLOCK: usize = 64
pub struct Sha256
pub const fn new() -> Self
pub fn update(&mut self, mut data: &[u8])
pub fn finish(mut self) -> [u8; 32]
pub fn sha256(data: &[u8]) -> [u8; 32]
pub const SHA512_LEN: usize = 64
pub struct Sha512
pub fn sha512(data: &[u8]) -> [u8; 64]
pub struct HmacSha256
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32]
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32]
pub fn hkdf_expand(prk: &[u8; 32], info: &[u8], okm: &mut [u8])
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool
```

`Sha256` and `Sha512` are incremental (`update`, then `finish`, which consumes the state: `clone` it first to read a running hash, as the TLS transcript does); `HmacSha256` likewise
(`HmacSha256::new(key)`, `update`, `finish`). `hkdf_expand` fills `okm` (at most 255 x 32 bytes) and panics beyond that. `constant_time_eq` compares MACs and tags without stopping at the first difference.

## ChaCha20-Poly1305

```rust,ignore
pub fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64]
pub fn chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &mut [u8])
pub struct Poly1305
pub fn poly1305(key: &[u8; 32], data: &[u8]) -> [u8; 16]
pub const TAG_LEN: usize = 16
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8]) -> [u8; 16]
pub fn open(
```

RFC 8439. `seal` encrypts `data` in place and returns the tag over the additional data and the ciphertext; `open(key, nonce, aad, data, tag)` checks the tag first and decrypts only if it is right (on a wrong tag
the data is left as it was and the result is `false`). **A (key, nonce) pair must never be used twice**: the TLS layer builds the nonce from a per-direction record counter. Poly1305 uses 26-bit limbs and the
ChaCha20 core only adds, rotates and xors, so neither has secret-dependent branches or table lookups.

## X25519 and Ed25519

```rust,ignore
pub const BASEPOINT: [u8; 32] =
pub fn x25519(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32]
pub fn x25519_public(private: &[u8; 32]) -> [u8; 32]
pub fn is_all_zero(secret: &[u8; 32]) -> bool
pub fn ed25519_public(seed: &[u8; 32]) -> [u8; 32]
pub fn ed25519_sign_with_public(seed: &[u8; 32], public: &[u8; 32], message: &[u8]) -> [u8; 64]
pub fn ed25519_sign(seed: &[u8; 32], message: &[u8]) -> [u8; 64]
pub fn ed25519_verify(public: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool
```

X25519 (RFC 7748) is the Montgomery ladder; clamping is done inside `x25519`. A peer point of low order gives the all-zero secret, which `is_all_zero` detects and TLS refuses. Ed25519 (RFC 8032) signs the message
directly (it is deterministic: no random numbers); `ed25519_sign_with_public` takes the public key to avoid recomputing it (about 44 ms on the PYNQ-Z2); `ed25519_verify` rejects a signature whose S is not below the
group order. The field arithmetic (sixteen 16-bit limbs in `i64`, the TweetNaCl representation) is small and constant time but not fast: on the PYNQ-Z2's 650 MHz core an X25519 takes about 26 ms and an Ed25519
signature or public key about 44 ms.

## ECDSA on P-256

```rust,ignore
pub fn p256_private_is_valid(private: &[u8; 32]) -> bool
pub fn p256_public(private: &[u8; 32]) -> Option<[u8; 65]>
pub fn ecdsa_p256_sign(private: &[u8; 32], hash: &[u8; 32]) -> Option<[u8; 64]>
pub fn sig_to_der(sig: &[u8; 64]) -> ([u8; 72], usize)
pub fn ecdsa_p256_verify(public: &[u8; 65], hash: &[u8; 32], sig: &[u8; 64]) -> bool
```

NIST P-256 (secp256r1), for the certificates browsers accept. `p256_public` returns the uncompressed SEC1 point (`04 || x || y`); `ecdsa_p256_sign` signs a SHA-256 digest with the **deterministic nonce of RFC 6979**
(so signing needs no random numbers and cannot leak the key through a bad one) and returns `r || s`; `sig_to_der` makes the `SEQUENCE { INTEGER r, INTEGER s }` that X.509 and TLS carry. The arithmetic is
Montgomery multiplication on eight 32-bit limbs with the complete addition formulas of Renes, Costello and Batina (2015), so the scalar multiplication runs the same operations whatever the key (no special cases for
doubling or the point at infinity). About 22 ms for a signature on the PYNQ-Z2. `ecdsa_p256_verify` (variable time, public data only) also checks that the point is on the curve.

## The random bit generator

```rust,ignore
pub struct Drbg
pub fn new(seed: &[u8]) -> Self
pub fn add_entropy(&mut self, data: &[u8])
pub fn fill(&mut self, out: &mut [u8])
pub fn bytes64(&mut self) -> [u8; 64]
```

ChaCha20 with fast key erasure: each `fill` replaces the key, so a later compromise of the state does not reveal earlier output, and `add_entropy` hashes new input into the key. **It stretches and mixes entropy; it creates none.**
The Zynq-7000 has no hardware random number generator, so all its security rests on what the program feeds it. What the HTTPS example feeds it, and why that is weak, is in the [TLS reference](tls.md#where-the-random-bytes-come-from).

## Testing

`cargo test -p hardroxide-crypto` runs the primitives against OpenSSL and Python's `cryptography` (`tests/gen_vectors.py` makes `tests/vectors.rs`, more than 800 random vectors with the answers of the independent implementations) and
against standard examples: both hashes on messages of every length around the block boundaries (and a million `a`s fed in uneven pieces); HMAC (RFC 4231) and HKDF (RFC 5869); ChaCha20-Poly1305 for 16 plaintext
lengths with 6 additional-data lengths and 60 random cases, checked byte for byte and for rejection of any altered tag, ciphertext or additional data; the RFC 8439 Poly1305 and ChaCha20 block examples; X25519 (RFC 7748
section 5.2 and 6.1, and 140 random exchanges); Ed25519 (RFC 8032 test 1 and 70 random keys and messages, public keys and signatures identical, tampering of every part caught, a non-canonical S refused); P-256
(64 public keys including the edge keys 1, 2, 3 and n-1; 70 RFC 6979 signatures **byte for byte equal to OpenSSL's**, the RFC 6979 appendix example; 40 randomized OpenSSL signatures verified; tampering caught; points off the
curve refused). The curve code was first tried against these and passed unchanged.

## What is not verified

That the "constant time" code is constant time on the real core: it avoids secret-dependent branches and indices by construction, but nobody has measured timing independence on the PYNQ-Z2 or looked at the generated
machine code, and the compiler is free to introduce branches. Fault attacks, power analysis and cache attacks (the A9 has caches; the code uses no secret-indexed tables, which is the main defence) are out of scope. There
has been no independent review.
