# Test TLS identity

`test-cert.der` / `test-cert.pem` and `test-seed.bin` are a **throwaway test credential**: a self-signed Ed25519 certificate (CN `hardroxide-test`) and the 32-byte private seed of its key, made with
`openssl req -x509 -newkey ed25519`. The private key is published in this repository, so it protects nothing: it exists so the QEMU test (`scripts/test-qemu.sh`) and the examples can run without making a
certificate first. **Never use it for anything real.** Make your own with `scripts/make-cert.sh` (see the HTTPS tutorial).
