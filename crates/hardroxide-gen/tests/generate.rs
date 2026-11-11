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
    std::fs::write(tmp.join("tests/mock.rs"), include_str!("data/generated_mock_test.rs")).unwrap();
    // Standalone package, not part of this workspace.
    let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
    manifest.push_str("\n[workspace]\n");
    std::fs::write(tmp.join("Cargo.toml"), manifest).unwrap();
    let out = Command::new(env!("CARGO"))
        .args(["test", "--manifest-path"])
        .arg(tmp.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root().join("target/gen-test"))
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{text}");
    assert!(!text.contains("warning"), "generated crate has warnings:\n{text}");
    std::fs::remove_dir_all(tmp).unwrap();
}
