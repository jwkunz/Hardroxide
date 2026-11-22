# Test TLS identities

Throwaway **test credentials**: self-signed certificates and the private keys behind them, made with OpenSSL. The private keys are published in this repository, so they protect nothing:
they exist so the QEMU test (`scripts/test-qemu.sh`), the host tests and the examples can run without making a certificate first. **Never use them for anything real.**
Make your own with `hardroxide make-cert` (see the HTTPS tutorial).

| Files | Key | For |
|---|---|---|
| `test-cert.der`, `test-cert.pem`, `test-key.bin` | ECDSA P-256 (the 32-byte private scalar) | the examples and the QEMU test; **browsers accept only this kind** (Chromium offers ECDSA and RSA signatures, not Ed25519) |
| `test-ed25519-cert.der`, `test-ed25519-cert.pem`, `test-ed25519-seed.bin` | Ed25519 (the 32-byte seed) | the host interoperability tests with `curl` and `openssl s_client` |
