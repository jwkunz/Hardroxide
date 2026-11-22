//! A deterministic random bit generator (ChaCha20 with "fast key erasure") that is seeded and reseeded with whatever entropy the caller can gather.
//!
//! **This is not a source of randomness; it only stretches and mixes the entropy it is given.** The Zynq-7000 has no hardware random number generator, so a
//! program must feed it from somewhere: a seed stored on the card and updated at every boot, the jitter of event timings, a seed made on the host. If the entropy fed in is
//! guessable, so is everything this produces, and with it the TLS session keys. See the reference page for what the examples feed it and what is wrong with that.

use crate::chacha20::chacha20_block;
use crate::sha256::Sha256;

/// The generator. `fill` produces output; `add_entropy` mixes in more input at any time.
pub struct Drbg {
    key: [u8; 32],
    calls: u64,
    /// How many bytes of entropy input have been mixed in (not a measure of quality: the caller says nothing about it).
    pub mixed_bytes: u64,
}

impl Drbg {
    /// A generator seeded with `seed` (any length, including none: then it is a fixed, guessable stream until entropy is added).
    pub fn new(seed: &[u8]) -> Self {
        let mut d = Drbg {
            key: [0; 32],
            calls: 0,
            mixed_bytes: 0,
        };
        d.add_entropy(seed);
        d
    }

    /// Mix `data` into the state: the new key is SHA-256 of the old key, a domain tag and the data.
    pub fn add_entropy(&mut self, data: &[u8]) {
        let mut h = Sha256::new();
        h.update(&self.key);
        h.update(b"hardroxide drbg add");
        h.update(data);
        self.key = h.finish();
        self.mixed_bytes += data.len() as u64;
    }

    /// Fill `out` with pseudorandom bytes. The key is replaced afterwards, so a later compromise of the state does not reveal earlier output.
    pub fn fill(&mut self, out: &mut [u8]) {
        self.calls += 1;
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&self.calls.to_be_bytes());
        // Block 0 is the next key; the output starts at block 1.
        let next = chacha20_block(&self.key, 0, &nonce);
        for (i, chunk) in out.chunks_mut(64).enumerate() {
            let block = chacha20_block(&self.key, 1 + i as u32, &nonce);
            chunk.copy_from_slice(&block[..chunk.len()]);
        }
        self.key.copy_from_slice(&next[..32]);
    }

    /// 64 fresh bytes, the amount a TLS handshake needs here.
    pub fn bytes64(&mut self) -> [u8; 64] {
        let mut b = [0u8; 64];
        self.fill(&mut b);
        b
    }
}
