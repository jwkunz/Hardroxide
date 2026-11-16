# Releasing

Release checklist for a maintainer. Nothing here is automated, and publishing is
irreversible (a crate version can be yanked but not deleted).

1. **State.** `main` is green: `cargo fmt --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo test --workspace`, `mdbook build`,
   `scripts/test-armv7.sh`, and the CI and docs workflows.
2. **Board run.** On a PYNQ-Z2, work through
   [Verification gaps](08-verification-gaps.md) again if drivers changed
   (the programs are in `fixtures/board_tests/`).
3. **Version.** Set `version` in the workspace `Cargo.toml` and the `version`
   of each `hardroxide-*` entry under `[workspace.dependencies]`; update
   `CHANGELOG.md`.
4. **Dry run.** `cargo publish --workspace --dry-run` packages and builds every
   crate against the others as they will appear on crates.io.
5. **Publish.** `cargo publish --workspace` publishes in dependency order
   (`hwh`, `mmio`, `fpga`, `dma`, `irq`, `ip`, `gen`, `cli`). It needs a
   crates.io API token (`cargo login`).
6. **Tag.** `git tag -a v1.0.0 -m "1.0.0" && git push origin v1.0.0`, and create
   a GitHub release from the changelog.
7. **Docs.** The `docs` workflow deploys the book to GitHub Pages on every push
   to `main`; check <https://jwkunz.github.io/Hardroxide/>.

Generated crates depend on the runtime crates by version (`"1"`) unless
`--hardroxide-path` is given, so a crate generated before publication only builds
with that flag.

The CLI is the `hardroxide-cli` crate; `cargo install hardroxide-cli` installs the
`hardroxide` binary. The crate name `hardroxide` itself is unclaimed.
