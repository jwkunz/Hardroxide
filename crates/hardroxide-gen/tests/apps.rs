//! The applications in `apps/` build (Linux ones: type-check) for their board's processor against a freshly generated crate of the platform
//! their directory is named after, from the board's base overlay.

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
    (
        "rfsoc4x2",
        "baremetal",
        Platform::BareMetal,
        "fixtures/board/rfsoc4x2_base_overlay.xsa",
    ),
    (
        "rfsoc4x2",
        "rtos",
        Platform::Rtos,
        "fixtures/board/rfsoc4x2_base_overlay.xsa",
    ),
    (
        "rfsoc4x2",
        "linux",
        Platform::Linux,
        "fixtures/board/rfsoc4x2_base_overlay.xsa",
    ),
];

#[test]
fn applications_build() {
    let targets = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    for &(set, dir, platform, design) in APPS {
        let ultrascale = set == "rfsoc4x2";
        let linux = platform == Platform::Linux;
        let target = if linux {
            "aarch64-unknown-linux-gnu"
        } else if ultrascale {
            "aarch64-unknown-none"
        } else {
            "armv7a-none-eabihf"
        };
        if !targets.contains(target) {
            eprintln!("skipping {set}/{dir}: `rustup target add {target}` to build it");
            continue;
        }
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
        // Data the applications include (`include_bytes!("chime.wav")`) sits next to them.
        let data = root().join("apps").join(set).join("data");
        if let Ok(rd) = std::fs::read_dir(&data) {
            for e in rd.flatten() {
                if e.path()
                    .extension()
                    .is_some_and(|x| x == "wav" || x == "der" || x == "bin")
                {
                    std::fs::copy(e.path(), tmp.join("examples").join(e.file_name())).unwrap();
                }
            }
        }
        let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
        // The PYNQ-Z2's applications use the library crates beyond what a generated crate depends on.
        let libraries = format!(
            "\n[dev-dependencies]\nhardroxide-gfx = {{ path = {:?} }}\nhardroxide-video = {{ path = {:?}, default-features = false }}\nhardroxide-audio = {{ path = {:?}, default-features = false }}\nhardroxide-fat = {{ path = {:?} }}\nhardroxide-net = {{ path = {:?} }}\nhardroxide-crypto = {{ path = {:?} }}\nhardroxide-tls = {{ path = {:?} }}\nhardroxide-ps7 = {{ path = {:?}, default-features = false, features = [\"net\"] }}\n\n[workspace]\n",
            root().join("crates/hardroxide-gfx"),
            root().join("crates/hardroxide-video"),
            root().join("crates/hardroxide-audio"),
            root().join("crates/hardroxide-fat"),
            root().join("crates/hardroxide-net"),
            root().join("crates/hardroxide-crypto"),
            root().join("crates/hardroxide-tls"),
            root().join("crates/hardroxide-ps7")
        );
        manifest.push_str(if ultrascale {
            "\n[workspace]\n"
        } else {
            &libraries
        });
        std::fs::write(tmp.join("Cargo.toml"), manifest).unwrap();
        // A Linux application is only checked: that needs no cross linker.
        let command: &[&str] = if linux {
            &["check", "--examples"]
        } else {
            &["build", "--release", "--examples"]
        };
        let out = Command::new(env!("CARGO"))
            .args(command)
            .args(["--target", target, "--manifest-path"])
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
