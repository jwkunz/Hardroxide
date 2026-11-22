# hardroxide-crypto

> **Warning: home-made and unaudited.** This cryptography library was written for education and for trusted networks. It matches test vectors and interoperates with OpenSSL, curl and a browser, but nobody outside this project has reviewed it,
> its timing behaviour has not been measured, and the Zynq-7000 has no random number generator, so the quality of anything built on it depends entirely on the randomness you supply. Do not use it to protect anything valuable. See the [security policy](https://github.com/jwkunz/Hardroxide/blob/main/SECURITY.md).

Cryptographic primitives written for [Hardroxide](https://github.com/jwkunz/Hardroxide), for the TLS 1.3 server of the `hardroxide-tls` crate: SHA-256 and SHA-512, HMAC-SHA-256,
HKDF, ChaCha20-Poly1305 (RFC 8439), X25519 (RFC 7748) and Ed25519 (RFC 8032). `no_std`, no allocation, no dependencies.

**This is home-made cryptography for education and for a trusted network. It has not been audited.** The secret-dependent code (the ChaCha20 and Poly1305 state, the field arithmetic and the
scalar multiplications) is written to run in constant time, with no secret-dependent branches or table lookups, but nothing here has been checked for side channels on real hardware. The test suite
compares every primitive with OpenSSL and with Python's `cryptography` on thousands of random inputs and with the standard test vectors. The user guide and reference are at
<https://jwkunz.github.io/Hardroxide/>.

Licensed under either of Apache-2.0 or MIT at your option.
