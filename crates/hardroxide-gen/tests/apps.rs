//! The applications in `apps/` build for the Cortex-A9 against a freshly generated crate of the
//! platform their directory is named after, from the board-ready PYNQ base overlay.

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

/// `(application set, platform dir, platform, design)`.
const APPS: &[(&str, &str, Platform, &str)] = &[
    (
        "base",
        "baremetal",
        Platform::BareMetal,
        "fixtures/board/pynq_base_overlay.xsa",
    ),
    (
        "base",
        "rtos",
        Platform::Rtos,
        "fixtures/board/pynq_base_overlay.xsa",
    ),
];

#[test]
fn applications_build() {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("armv7a-none-eabihf"))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipping: `rustup target add armv7a-none-eabihf` to run this test");
        return;
    }
    for &(set, dir, platform, design) in APPS {
        let tmp = std::env::temp_dir().join(format!("hrx-app-{set}-{dir}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let o = Options {
            platform,
            crate_name: Some("design_1".into()),
            hardroxide_path: Some(root()),
        };
        let xsa = Xsa::open(root().join(design)).unwrap();
        generate(&xsa, &o).unwrap().write_to(&tmp).unwrap();
        let src = root().join("apps").join(set).join(dir);
        let mut n = 0;
        for e in std::fs::read_dir(&src).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "rs") {
                std::fs::copy(&p, tmp.join("examples").join(p.file_name().unwrap())).unwrap();
                n += 1;
            }
        }
        assert!(n > 0, "no applications in {}", src.display());
        let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
        // The applications use the library crates beyond what a generated crate depends on.
        manifest.push_str(&format!(
            "\n[dev-dependencies]\nhardroxide-gfx = {{ path = {:?} }}\nhardroxide-video = {{ path = {:?}, default-features = false }}\n\n[workspace]\n",
            root().join("crates/hardroxide-gfx"),
            root().join("crates/hardroxide-video")
        ));
        std::fs::write(tmp.join("Cargo.toml"), manifest).unwrap();
        let out = Command::new(env!("CARGO"))
            .args([
                "build",
                "--release",
                "--examples",
                "--target",
                "armv7a-none-eabihf",
                "--manifest-path",
            ])
            .arg(tmp.join("Cargo.toml"))
            .env(
                "CARGO_TARGET_DIR",
                root().join(format!("target/gen-test-apps/{set}-{dir}")),
            )
            .output()
            .unwrap();
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "{set}/{dir}:\n{text}");
        assert!(!text.contains("warning"), "{set}/{dir}: warnings:\n{text}");
        std::fs::remove_dir_all(tmp).unwrap();
    }
}

/// The board-ready base overlay carries what `hardroxide run` needs.
#[test]
fn board_ready_base_overlay_has_ps7_init() {
    let p = root().join("fixtures/board/pynq_base_overlay.xsa");
    let z = std::fs::File::open(p).unwrap();
    let mut a = zip_names(z);
    a.sort();
    assert!(a.iter().any(|n| n == "ps7_init.tcl"), "{a:?}");
}

fn zip_names(f: std::fs::File) -> Vec<String> {
    use std::io::Read;
    // The XSA is a zip: list the names without a zip dependency by scanning local headers.
    let mut bytes = Vec::new();
    std::io::BufReader::new(f).read_to_end(&mut bytes).unwrap();
    let mut names = Vec::new();
    let mut i = 0;
    while i + 30 <= bytes.len() {
        if bytes[i..i + 4] == [0x50, 0x4b, 0x03, 0x04] {
            let n = u16::from_le_bytes([bytes[i + 26], bytes[i + 27]]) as usize;
            let m = u16::from_le_bytes([bytes[i + 28], bytes[i + 29]]) as usize;
            if let Ok(name) = std::str::from_utf8(&bytes[i + 30..i + 30 + n]) {
                names.push(name.to_string());
            }
            i += 30 + n + m;
        } else {
            i += 1;
        }
    }
    names
}
