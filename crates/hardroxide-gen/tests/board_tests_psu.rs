//! Compiles the on-board test programs for Zynq UltraScale+ (`fixtures/board_tests_rfsoc/`) against the crates
//! generated for their designs, without warnings.

use hardroxide_gen::{Options, Platform, generate};
use hardroxide_hwh::Xsa;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn board_test_programs_compile() {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("aarch64-unknown-none"))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipping: `rustup target add aarch64-unknown-none` to run this test");
        return;
    }
    let programs = root().join("fixtures/board_tests_rfsoc");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&programs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert_eq!(
        dirs.len(),
        7,
        "one directory for each design with a program"
    );
    for dir in dirs {
        let name = dir.file_name().unwrap().to_str().unwrap().to_string();
        let xsa = if name == "base" {
            "fixtures/board/rfsoc4x2_base_overlay.xsa".to_string()
        } else {
            format!("fixtures/rfsoc4x2/rfsoc4x2_{name}.xsa")
        };
        let tmp = std::env::temp_dir().join(format!("hrx-psu-bt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let opts = Options {
            platform: Platform::BareMetal,
            crate_name: Some("design_1".into()),
            hardroxide_path: Some(root()),
        };
        generate(&Xsa::open(root().join(xsa)).unwrap(), &opts)
            .unwrap()
            .write_to(&tmp)
            .unwrap();
        for file in std::fs::read_dir(&dir).unwrap() {
            let file = file.unwrap().path();
            std::fs::copy(&file, tmp.join("examples").join(file.file_name().unwrap())).unwrap();
        }
        let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
        manifest.push_str("\n[workspace]\n");
        std::fs::write(tmp.join("Cargo.toml"), manifest).unwrap();
        let out = Command::new(env!("CARGO"))
            .args([
                "build",
                "--release",
                "--examples",
                "--target",
                "aarch64-unknown-none",
                "--manifest-path",
            ])
            .arg(tmp.join("Cargo.toml"))
            .env(
                "CARGO_TARGET_DIR",
                root().join(format!("target/gen-test-psu-board/{name}")),
            )
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "{name}:\n{text}");
        assert!(!text.contains("warning"), "{name}: warnings:\n{text}");
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
