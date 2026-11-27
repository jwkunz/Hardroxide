# Contributing

Thank you for looking. A few things make a change easy to accept.

- **Read first:** `docs/design-index.md` and the decision records in `docs/decisions/`; the book (`docs/`) is the authoritative documentation. When you change a public API or the CLI, update `docs/reference/` in the same change
  (`cargo test -p hardroxide-cli --test book` fails if signatures, help text or links drift).
- **Checks** (CI runs them with the newest stable Rust, whose clippy is stricter than an old one: `rustup update` first):
  `scripts/check-compat.py` (backward compatibility with the last release; needs `cargo-semver-checks`), `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/check-no-std.sh`, and `scripts/test-qemu.sh` for anything touching the PS support, the kernel, the SD card or the network.
  Needs `dosfstools`, `curl`, `openssl` and Python 3 for the QEMU script.
- **Hardware:** results from a board are recorded in `docs/08-verification-gaps.md` with what was *not* run; please do not claim a board result you did not capture.
- **The runtime crates stay `no_std`** (see `CLAUDE.md` for the conventions); generated code must be deterministic.
- **Cryptography and TLS:** a change to `hardroxide-crypto` or `hardroxide-tls` needs a test that fails without it (a vector, a differential test against OpenSSL, or an interoperability test) and must not add a secret-dependent branch or table lookup.
- **Licence:** contributions are accepted under the project's licence, `MIT OR Apache-2.0` (see the licence section of the README), unless you say otherwise before it is merged.
- **Security problems:** report them privately, see `SECURITY.md`.
