//! The bare-metal platform: golden snapshots of the generated sources, and a check that
//! every fixture's crate builds for the Cortex-A9 (`armv7a-none-eabihf`).

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

fn fixture(n: &str) -> Xsa {
    Xsa::open(root().join(format!("fixtures/{n}.xsa"))).unwrap()
}

fn opts(path: &str) -> Options {
    Options {
        platform: Platform::BareMetal,
        hardroxide_path: Some(path.into()),
        ..Default::default()
    }
}

const FIXTURES: &[&str] = &[
    "pynq_z2_gpio",
    "pynq_z2_peripherals",
    "pynq_z2_hls_vadd",
    "pynq_z2_hls_scale",
    "pynq_z2_dma_loopback",
    "pynq_z2_intc",
    "pynq_z2_mem_uart",
    "pynq_z2_fifo",
    "pynq_z2_loopback",
];

/// Snapshot of the generated sources for two fixtures that between them cover timers,
/// interrupts, GPIO and HLS. Regenerate with `UPDATE_GOLDEN=1 cargo test -p hardroxide-gen`.
#[test]
fn golden_snapshot() {
    for name in ["pynq_z2_gpio", "pynq_z2_peripherals", "pynq_z2_hls_vadd"] {
        let g = generate(&fixture(name), &opts("/hardroxide")).unwrap();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden_baremetal")
            .join(name);
        for (rel, data) in &g.files {
            let p = dir.join(rel);
            if std::env::var_os("UPDATE_GOLDEN").is_some() {
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(&p, data).unwrap();
            } else {
                let want =
                    std::fs::read(&p).unwrap_or_else(|_| panic!("missing golden {}", p.display()));
                assert_eq!(
                    String::from_utf8_lossy(data),
                    String::from_utf8_lossy(&want),
                    "{name}/{}",
                    rel.display()
                );
            }
        }
    }
}

#[test]
fn deterministic_and_has_no_bitstream() {
    let g = generate(&fixture("pynq_z2_gpio"), &opts("/h")).unwrap();
    assert_eq!(g, generate(&fixture("pynq_z2_gpio"), &opts("/h")).unwrap());
    // The loader takes the bitstream from the XSA; the crate embeds none.
    assert!(!g.files.keys().any(|k| k.starts_with("bitstream")));
    assert!(g.files.contains_key(Path::new("link.ld")));
    let manifest = String::from_utf8_lossy(&g.files[Path::new("Cargo.toml")]).into_owned();
    assert!(manifest.contains("(platform: baremetal)"));
}

#[test]
fn rtos_crate_depends_on_the_kernel_and_has_only_kernel_examples() {
    let o = Options {
        platform: Platform::Rtos,
        hardroxide_path: Some("/h".into()),
        ..Default::default()
    };
    let g = generate(&fixture("pynq_z2_gpio"), &o).unwrap();
    let manifest = String::from_utf8_lossy(&g.files[Path::new("Cargo.toml")]).into_owned();
    assert!(manifest.contains("(platform: rtos)"));
    assert!(manifest.contains("hardroxide-rt"));
    // The kernel supplies the interrupt entries, so the PS crate's defaults are off.
    let ps7 = manifest
        .lines()
        .find(|l| l.starts_with("hardroxide-ps7"))
        .unwrap();
    assert!(ps7.contains("default-features = false"), "{ps7}");
    assert!(g.files.contains_key(Path::new("examples/rt_control.rs")));
    assert!(!g.files.contains_key(Path::new("examples/board_check.rs")));
}

/// Generate every fixture as a bare-metal crate and build its library and examples for the
/// Cortex-A9. Skipped (with a note) if the target is not installed.
#[test]
fn every_fixture_builds_for_the_target() {
    build_all(Platform::BareMetal);
}

/// The same for the RTOS flavour (the kernel and its example).
#[test]
fn every_fixture_builds_with_the_kernel() {
    build_all(Platform::Rtos);
}

fn build_all(platform: Platform) {
    let have = Command::new("rustc")
        .args(["--print", "target-list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("armv7a-none-eabihf"))
        .unwrap_or(false);
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("armv7a-none-eabihf"))
        .unwrap_or(false);
    if !have || !installed {
        eprintln!("skipping: `rustup target add armv7a-none-eabihf` to run this test");
        return;
    }
    for name in FIXTURES {
        let tmp =
            std::env::temp_dir().join(format!("hrx-bm-{name}-{platform:?}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let o = Options {
            platform,
            crate_name: Some("design_1".into()),
            ..opts(root().to_str().unwrap())
        };
        generate(&fixture(name), &o)
            .unwrap()
            .write_to(&tmp)
            .unwrap();
        // Standalone package, not part of this workspace.
        let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
        manifest.push_str("\n[workspace]\n");
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
            .env("CARGO_TARGET_DIR", root().join("target/gen-test-baremetal"))
            .output()
            .unwrap();
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "{name}:\n{text}");
        assert!(
            !text.contains("warning"),
            "{name}: generated crate has warnings:\n{text}"
        );
        std::fs::remove_dir_all(tmp).unwrap();
    }
}

/// `update` on a bare-metal crate: nothing to replace when the interface is unchanged, a diff
/// when it changed, and `--regen` keeps the manifest and the (possibly tuned) linker script.
#[test]
fn update_understands_baremetal_crates() {
    use hardroxide_gen::update::{Outcome, update};
    let dir = std::env::temp_dir().join(format!("hrx-bm-upd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let x = fixture("pynq_z2_gpio");
    generate(&x, &opts("/h")).unwrap().write_to(&dir).unwrap();
    assert!(matches!(
        update(&dir, &x, false).unwrap(),
        Outcome::Unchanged
    ));
    // A different design (other interface): refused without --regen, regenerated with it.
    let y = fixture("pynq_z2_peripherals");
    assert!(matches!(
        update(&dir, &y, false).unwrap(),
        Outcome::InterfaceChanged { .. }
    ));
    std::fs::write(dir.join("link.ld"), "/* tuned by the user */\n").unwrap();
    assert!(matches!(
        update(&dir, &y, true).unwrap(),
        Outcome::Regenerated { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(dir.join("link.ld")).unwrap(),
        "/* tuned by the user */\n"
    );
    assert!(dir.join("examples/timer_delay.rs").exists());
    let _ = std::fs::remove_dir_all(dir);
}

/// The programs the bare-metal and RTOS tutorials show (`fixtures/board_tests_bm/`) build
/// against a freshly generated crate of the platform their directory is named after.
#[test]
fn tutorial_programs_build() {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("armv7a-none-eabihf"))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipping: `rustup target add armv7a-none-eabihf` to run this test");
        return;
    }
    for (dir, platform, xsa) in [
        ("baremetal_gpio", Platform::BareMetal, "pynq_z2_gpio"),
        ("rtos_gpio", Platform::Rtos, "pynq_z2_gpio"),
    ] {
        let tmp = std::env::temp_dir().join(format!("hrx-tut-{dir}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let o = Options {
            platform,
            crate_name: Some("design_1".into()),
            ..opts(root().to_str().unwrap())
        };
        generate(&fixture(xsa), &o).unwrap().write_to(&tmp).unwrap();
        let src = root().join("fixtures/board_tests_bm").join(dir);
        for e in std::fs::read_dir(&src).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "rs") {
                std::fs::copy(&p, tmp.join("examples").join(p.file_name().unwrap())).unwrap();
            }
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
                "armv7a-none-eabihf",
                "--manifest-path",
            ])
            .arg(tmp.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", root().join("target/gen-test-baremetal"))
            .output()
            .unwrap();
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "{dir}:\n{text}");
        assert!(!text.contains("warning"), "{dir}: warnings:\n{text}");
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
