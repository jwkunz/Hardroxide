use hardroxide_gen::{generate, Options};
use hardroxide_hwh::Xsa;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn fixture() -> Xsa {
    Xsa::open(root().join("fixtures/pynq_z2_gpio.xsa")).unwrap()
}

#[test]
fn deterministic() {
    let o = Options::default();
    assert_eq!(generate(&fixture(), &o).unwrap(), generate(&fixture(), &o).unwrap());
}

/// Golden snapshot of the generated sources. Regenerate with
/// `UPDATE_GOLDEN=1 cargo test -p hardroxide-gen`.
#[test]
fn golden_snapshot() {
    let opts = Options { hardroxide_path: Some("/hardroxide".into()), ..Default::default() };
    let g = generate(&fixture(), &opts).unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    for (rel, data) in &g.files {
        if rel.extension().is_some_and(|e| e == "bin") {
            continue; // big; covered by BITSTREAM_SHA256 in lib.rs
        }
        let p = dir.join(rel);
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, data).unwrap();
        } else {
            let want = std::fs::read(&p).unwrap_or_else(|_| panic!("missing golden {}", p.display()));
            assert_eq!(String::from_utf8_lossy(data), String::from_utf8_lossy(&want), "{}", rel.display());
        }
    }
}

/// Generate the crate, compile it, and run a mock-MMIO test against it.
#[test]
fn generated_crate_builds_and_works_with_mock() {
    let tmp = std::env::temp_dir().join(format!("hrx-gen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let opts = Options { hardroxide_path: Some(root()), crate_name: Some("pynq_z2_gpio_test".into()) };
    let g = generate(&fixture(), &opts).unwrap();
    g.write_to(&tmp).unwrap();
    std::fs::create_dir_all(tmp.join("tests")).unwrap();
    std::fs::write(tmp.join("tests/mock.rs"), include_str!("data/generated_mock_test.rs")
            .replace("__FIXTURE__", root().join("fixtures/pynq_z2_gpio.xsa").to_str().unwrap())).unwrap();
    // Standalone package, not part of this workspace.
    let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
    manifest.push_str("\n[workspace]\n");
    std::fs::write(tmp.join("Cargo.toml"), manifest).unwrap();
    let out = Command::new(env!("CARGO"))
        .args(["test", "--features", "dev-bitstream", "--manifest-path"])
        .arg(tmp.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root().join("target/gen-test"))
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{text}");
    assert!(!text.contains("warning"), "generated crate has warnings:\n{text}");
    std::fs::remove_dir_all(tmp).unwrap();
}

fn fresh_crate(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hrx-upd-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    generate(&fixture(), &Options::default()).unwrap().write_to(&dir).unwrap();
    dir
}

fn snapshot(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut m = std::collections::BTreeMap::new();
    fn walk(base: &Path, d: &Path, m: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() { walk(base, &p, m) } else { m.insert(p.strip_prefix(base).unwrap().into(), std::fs::read(&p).unwrap()); }
        }
    }
    walk(dir, dir, &mut m);
    m
}

#[test]
fn update_same_xsa_is_unchanged() {
    use hardroxide_gen::update::{Outcome, update};
    let dir = fresh_crate("same");
    assert_eq!(update(&dir, &fixture(), false).unwrap(), Outcome::Unchanged);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn update_bitstream_only_touches_only_bitstream_files() {
    use hardroxide_gen::update::{Outcome, update};
    let dir = fresh_crate("bit");
    let before = snapshot(&dir);
    let mut x = fixture();
    let n = x.bit.len();
    x.bit[n - 100] ^= 0xff; // same interface, different payload
    assert!(matches!(update(&dir, &x, false).unwrap(), Outcome::BitstreamReplaced { .. }));
    let after = snapshot(&dir);
    let changed: Vec<_> = after.iter().filter(|(k, v)| before.get(*k) != Some(v)).map(|(k, _)| k.to_str().unwrap()).collect();
    assert_eq!(changed, ["bitstream/design.bin", "bitstream/design.sha256"]);
    assert_eq!(after.len(), before.len());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn update_interface_change_needs_regen() {
    use hardroxide_gen::update::{DiffLine, Outcome, update};
    let dir = fresh_crate("iface");
    let mut x = fixture();
    x.design.memory_ranges[0].base = 0x4300_0000;
    x.design.memory_ranges[0].high = 0x4300_FFFF;
    let before = snapshot(&dir);
    let Outcome::InterfaceChanged { diff } = update(&dir, &x, false).unwrap() else { panic!() };
    assert!(diff.iter().any(|d| matches!(d, DiffLine::Added(l) if l.contains("0x43000000"))));
    assert_eq!(snapshot(&dir), before, "no files touched without --regen");
    assert!(matches!(update(&dir, &x, true).unwrap(), Outcome::Regenerated { .. }));
    let pac = std::fs::read_to_string(dir.join("src/pac.rs")).unwrap();
    assert!(pac.contains("0x43000000"));
    assert_eq!(std::fs::read_to_string(dir.join("Cargo.toml")).unwrap(), String::from_utf8(before[Path::new("Cargo.toml")].clone()).unwrap());
    assert_eq!(update(&dir, &x, false).unwrap(), Outcome::Unchanged);
    std::fs::remove_dir_all(dir).unwrap();
}
