# Releasing

Release checklist for a maintainer. Nothing here is automated, and publishing is
irreversible (a crate version can be yanked but not deleted).

1. **State.** `main` is green: `cargo fmt --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/build-site.sh` (the book and rustdoc),
   `scripts/test-armv7.sh`, `scripts/test-qemu.sh` (bare metal and kernel) and the CI and docs workflows.
2. **Board run.** On a PYNQ-Z2, work through
   [Verification gaps](08-verification-gaps.md) again if drivers changed
   (the programs are in `fixtures/board_tests/`). For the bare-metal and RTOS flavors, run `baremetal/rt-smoke`,
   `baremetal/rt-jitter` and the tutorial programs in `fixtures/board_tests_bm/` over JTAG.
3. **Version.** Set `version` in the workspace `Cargo.toml` and the `version`
   of each `hardroxide-*` entry under `[workspace.dependencies]`; update
   `CHANGELOG.md`.
4. **Dry run.** `cargo publish --workspace --dry-run` packages and builds every
   crate against the others as they will appear on crates.io (it does not need a token). A crate's README and `repository` and `homepage` fields are fixed for that version once published, so check them here.
5. **Publish.** All 18 crates (17 until 3.0), in dependency order (a crate must be on the index before one that depends on it can be published):
   `hardroxide-mmio`, `-hwh`, `-irq`, `-crypto`, `-net`, `-gfx`, `-dma`, `-fat`, `-fpga`, `-ps7`, `-psu` (new in 3.0), `-tls`, `-gen`, `-ip`, `-rt`, `-audio`, `-video`, `-cli`.
   Log in with `cargo login` (a token scoped to `publish-new` and `publish-update` for `hardroxide*`, with a short expiry; never paste a token into a chat or a file in the repository).
   **crates.io limits how fast new crates can be published (at the time of writing a burst of five, then one every ten minutes; check its current policy)**, so a first release of 17 took about two hours: publish one crate at a time (`cargo publish -p <crate>`) and, on a rate-limit
   error, wait for the time it names and go on. (`cargo publish --workspace` does the ordering but stops at the first error.) Publishing is irreversible: check the dry run first.
6. **Tag.** `git tag -a v2.0.0 -m "2.0.0" && git push origin v2.0.0`, and create
   a GitHub release from the changelog.
7. **Docs.** The `docs` workflow deploys the landing page, the book (`/book/`) and rustdoc (`/api/`) to GitHub Pages on every push
   to `main`; check <https://jwkunz.github.io/Hardroxide/>.

Generated crates depend on the runtime crates by version (`"2"`) unless
`--hardroxide-path` is given. All 17 crates were first published at 2.7.0 on 2026-11-22 (the `v2.7.0` tag is the commit before the security policy and the README warnings were added, so
the published crates differ from the tag by documentation files only).

The CLI is the `hardroxide-cli` crate; `cargo install hardroxide-cli` installs the
`hardroxide` binary. The crate name `hardroxide` itself is unclaimed.
