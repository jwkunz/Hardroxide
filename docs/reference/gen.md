# hardroxide-gen

The code generator and updater behind `hardroxide generate` / `update`.

```rust,ignore
pub fn generate(xsa: &Xsa, opts: &Options) -> Result<GeneratedCrate, GenError>
pub struct Options
pub crate_name: Option<String>,
pub hardroxide_path: Option<PathBuf>,
pub struct GeneratedCrate
pub files: BTreeMap<PathBuf, Vec<u8>>,
pub fn write_to(&self, dir: &Path) -> Result<(), GenError>
```

`generate` is a pure function of its inputs: the returned map is sorted by
relative path and contains the files listed in
[Generated crate](generated-crate.md). `Options::crate_name` overrides the
package name (default: the block design name); `hardroxide_path`, when set,
makes the dependency lines use `path = "<root>/crates/<crate>"` instead of a
crates.io version. `write_to` creates directories as needed and overwrites
files.

`GenError`: `Bit(BitError)`, `Xsa(hardroxide_hwh::Error)`, `Io { path, source }`.

## Updating

```rust,ignore
pub fn update(crate_dir: &Path, xsa: &Xsa, regen: bool) -> Result<Outcome, GenError>
pub fn diff_interface(old: &str, new: &str) -> Vec<DiffLine>
```

`update` compares the crate's stored `interface.txt` with the new XSA's
[`interface_text`](hwh.md#interface-hash) and returns an `Outcome`:

| Variant | Meaning |
|---|---|
| `Unchanged` | interface, bitstream and load config all identical; nothing written |
| `BitstreamReplaced { old_sha256, new_sha256 }` | only `bitstream/design.bin`, `design.sha256`, `load.cfg` rewritten |
| `InterfaceChanged { diff }` | interface differs and `regen` was false; nothing written |
| `Regenerated { diff }` | interface differed; everything but `Cargo.toml` regenerated |

`diff_interface` returns the lines only in `old` as `DiffLine::Removed` and
the lines only in `new` as `DiffLine::Added` (a set difference over lines, not
a positional diff). `DiffLine` implements `Display` as `- line` / `+ line`.
With `regen`, the package name is read back from the existing `Cargo.toml`,
which is kept untouched, and a stale `examples/gpio_smoke.rs` is removed if the
design no longer has a GPIO.
