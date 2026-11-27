# Releasing

Release checklist for a maintainer. Publishing is irreversible (a crate version can be yanked but not deleted), and
nothing here publishes by itself.

1. **State.** The commit to release is green in the CI workflow (format, clippy, tests, the book, `no_std`, QEMU, emulated
   ARM and AArch64, compatibility with 2.7.0). Run `scripts/build-site.sh` too: the docs workflow runs it only on `main`.
2. **Board run.** The release regression is the CI checks and one unattended run on the RFSoC 4x2
   ([ADR 0005](decisions/0005-ultrascale-plus.md), decision 11): JTAG boot mode, an Ethernet cable to the host, SMA cables
   from DAC A to ADC A and DAC B to ADC B, then `HRX_NET=1 HRX_RF_LOOPBACK=1 scripts/test-board-rfsoc.sh`. If code that
   runs on the Zynq-7000 changed since the PYNQ-Z2 was last run, run `scripts/test-board.sh` on it as well. Record the
   result, and what was not run, in [Verification gaps](08-verification-gaps.md).
3. **Version.** Set `version` in the workspace `Cargo.toml` and the `version` of each `hardroxide-*` entry under
   `[workspace.dependencies]`, refresh the lock files (`cargo metadata` in the root and in each directory under
   `baremetal/`), and date the `CHANGELOG.md` entry. Generated crates ask crates.io for the major version of the
   generator that wrote them, without further edits.
4. **Dry run.** `cargo publish --workspace --dry-run` packages and builds every crate against the others as they will
   appear on crates.io (it does not need a token). A crate's README and `repository` and `homepage` fields are fixed for
   that version once published, so check them here.
5. **Merge.** Merge the release branch into `main` and push; wait for the CI and docs workflows on `main`. The docs
   workflow deploys the landing page, the book (`/book/`) and rustdoc (`/api/`) to
   <https://jwkunz.github.io/Hardroxide/> at once, so publish soon after: the book describes the version being released.
6. **Publish**, from a clean checkout of that `main` commit. All 19 crates, in dependency order (a crate must be on the
   index before one that depends on it can be published): `hardroxide-mmio`, `-board`, `-hwh`, `-irq`, `-crypto`,
   `-net`, `-gfx`, `-dma`, `-fat`, `-fpga`, `-ps7`, `-psu`, `-tls`, `-gen`, `-ip`, `-rt`, `-audio`, `-video`, `-cli`.
   `cargo publish --workspace` does the ordering. Log in with `cargo login` first (a token scoped to `publish-new` and
   `publish-update` for `hardroxide*`, with a short expiry; never paste a token into a chat or a file in the repository).
   crates.io limits how fast crates can be published, new ones much more strictly than new versions (check its current
   policy); `-board` and `-psu` are new in 3.0. `cargo publish` stops at the first error: on a rate limit, wait for the
   time it names, then publish what is left one at a time (`cargo publish -p <crate>`), in the order above.
7. **Tag.** `git tag -a v3.0.0 -m "3.0.0" && git push origin v3.0.0` (with the release's version), and create a GitHub
   release from the changelog entry.
8. **Check.** `cargo install hardroxide-cli` gives the new version, a freshly generated crate builds without
   `--hardroxide-path`, and the pages site shows the new book.

Earlier releases: 3.0.0 on 2026-11-27 (19 crates). 2.7.0, yanked that day, was the first publication: 17 crates on 2026-11-22 (the `v2.7.0` tag is the commit before the
security policy and the README warnings were added, so the published crates differ from the tag by documentation files
only).

The CLI is the `hardroxide-cli` crate; `cargo install hardroxide-cli` installs the `hardroxide` binary. The crate name
`hardroxide` itself is unclaimed.
