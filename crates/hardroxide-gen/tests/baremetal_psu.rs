//! Bare metal and RTOS on Zynq UltraScale+: golden snapshots of the generated sources, a check that every RFSoC 4x2
//! design's crate builds for the Cortex-A53 (`aarch64-unknown-none`), and what the generator refuses.

use hardroxide_gen::update::{Outcome, update};
use hardroxide_gen::{GenError, Options, Platform, generate, summarize};
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

const BASE: &str = "board/rfsoc4x2_base_overlay";
const FIXTURES: &[&str] = &[
    BASE,
    "rfsoc4x2/rfsoc4x2_gpio",
    "rfsoc4x2/rfsoc4x2_intc",
    "rfsoc4x2/rfsoc4x2_loopback",
    "rfsoc4x2/rfsoc4x2_dma",
    "rfsoc4x2/rfsoc4x2_hls_vadd",
    "rfsoc4x2/rfsoc4x2_hls_scale",
    "rfsoc4x2/rfsoc4x2_rf_loopback",
];

fn opts(platform: Platform, path: &str) -> Options {
    Options {
        platform,
        hardroxide_path: Some(path.into()),
        ..Default::default()
    }
}

fn text<'a>(g: &'a hardroxide_gen::GeneratedCrate, file: &str) -> std::borrow::Cow<'a, str> {
    String::from_utf8_lossy(&g.files[Path::new(file)])
}

/// Between them: interrupts on the first port, on both ports, an HLS kernel with 64-bit pointers and GPIO. Regenerate
/// with `UPDATE_GOLDEN=1 cargo test -p hardroxide-gen`.
#[test]
fn golden_snapshot() {
    for name in ["rfsoc4x2_gpio", "rfsoc4x2_dma", "rfsoc4x2_hls_scale"] {
        let xsa = fixture(&format!("rfsoc4x2/{name}"));
        let g = generate(&xsa, &opts(Platform::BareMetal, "/hardroxide")).unwrap();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden_baremetal_psu")
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
fn the_crate_is_for_the_a53_and_names_this_familys_support() {
    let g = generate(&fixture(BASE), &opts(Platform::BareMetal, "/h")).unwrap();
    assert_eq!(
        g,
        generate(&fixture(BASE), &opts(Platform::BareMetal, "/h")).unwrap()
    );
    assert!(!g.files.keys().any(|k| k.starts_with("bitstream")));
    assert!(text(&g, ".cargo/config.toml").contains("aarch64-unknown-none"));
    for (file, data) in &g.files {
        let data = String::from_utf8_lossy(data);
        assert!(
            !data.contains("hardroxide_ps7") && !data.contains("ps7::"),
            "{}",
            file.display()
        );
    }
    let lib = text(&g, "src/lib.rs");
    // The console is the UART the design enables, and the overlay's interrupt controller reaches pl_ps_irq0[0].
    assert!(lib.contains("CONSOLE_UART: Option<usize> = Some(0xff010000);"));
    assert!(lib.contains("CONSOLE_UART_NUMBER: usize = 1;"));
    assert!(lib.contains("pub use hardroxide_psu as ps;"));
    assert!(text(&g, "src/pac.rs").contains("f2p_bit: Some(0), gic_spi: Some(89)"));
    // From crates.io the crates are the 3.x ones.
    let published = generate(
        &fixture(BASE),
        &Options {
            platform: Platform::BareMetal,
            ..Default::default()
        },
    )
    .unwrap();
    let manifest = text(&published, "Cargo.toml");
    assert!(manifest.contains("hardroxide-psu = { version = \"3\", features"));
    assert!(!manifest.contains("version = \"2\""));
}

#[test]
fn both_interrupt_ports_are_numbered() {
    let xsa = fixture("rfsoc4x2/rfsoc4x2_dma");
    let g = generate(&xsa, &opts(Platform::BareMetal, "/h")).unwrap();
    let pac = text(&g, "src/pac.rs");
    // pl_ps_irq0[0] is GIC ID 121 (SPI cell 89), pl_ps_irq1[0] ID 136 (cell 104).
    for want in [
        "f2p_bit: Some(0), gic_spi: Some(89)",
        "f2p_bit: Some(9), gic_spi: Some(105)",
    ] {
        assert!(pac.contains(want), "{want}");
    }
    let lines: Vec<String> = summarize(&xsa.design)
        .instances
        .into_iter()
        .flat_map(|i| i.interrupts)
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("pl_ps_irq1[1] (GIC SPI 105)")),
        "{lines:?}"
    );
}

#[test]
fn dma_engines_are_told_their_address_width() {
    let xsa = fixture("rfsoc4x2/rfsoc4x2_dma");
    let g = generate(&xsa, &opts(Platform::BareMetal, "/h")).unwrap();
    let lib = text(&g, "src/lib.rs");
    assert!(lib.contains(".with_address_width(32),") && lib.contains(".with_address_width(40),"));
    assert!(text(&g, "interface.txt").contains(" config c_addr_width=40"));
    // The Zynq-7000's interface text, and so its hash, is as before.
    let zynq = fixture("pynq_z2_dma_loopback");
    assert_eq!(zynq.design.dma_address_width("axi_dma_0"), None);
    assert!(!zynq.design.interface_text().contains("c_addr_width"));
}

#[test]
fn the_kernel_flavour_turns_the_processor_crates_entries_off() {
    let g = generate(&fixture(BASE), &opts(Platform::Rtos, "/h")).unwrap();
    let manifest = text(&g, "Cargo.toml");
    assert!(manifest.contains("(platform: rtos)") && manifest.contains("hardroxide-rt"));
    let psu = manifest
        .lines()
        .find(|l| l.starts_with("hardroxide-psu"))
        .unwrap();
    assert!(psu.contains("default-features = false"), "{psu}");
    assert!(g.files.contains_key(Path::new("examples/rt_control.rs")));
    assert!(!g.files.contains_key(Path::new("examples/board_check.rs")));
}

#[test]
fn linux_is_refused_for_now() {
    let e = generate(&fixture(BASE), &Options::default()).unwrap_err();
    assert!(matches!(e, GenError::UnsupportedArch { .. }));
    assert!(e.to_string().contains("--platform baremetal"), "{e}");
}

#[test]
fn update_keeps_a_crate_in_its_family() {
    let tmp = std::env::temp_dir().join(format!("hrx-psu-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let gpio = fixture("rfsoc4x2/rfsoc4x2_gpio");
    generate(&gpio, &opts(Platform::BareMetal, "/h"))
        .unwrap()
        .write_to(&tmp)
        .unwrap();
    assert_eq!(update(&tmp, &gpio, false).unwrap(), Outcome::Unchanged);
    // Another design of the same family: a diff, then a regeneration.
    let intc = fixture("rfsoc4x2/rfsoc4x2_intc");
    assert!(matches!(
        update(&tmp, &intc, false).unwrap(),
        Outcome::InterfaceChanged { .. }
    ));
    assert!(matches!(
        update(&tmp, &intc, true).unwrap(),
        Outcome::Regenerated { .. }
    ));
    // A Zynq-7000 design cannot take over the crate.
    let zynq = fixture("pynq_z2_gpio");
    assert!(matches!(
        update(&tmp, &zynq, true),
        Err(GenError::UnsupportedArch { .. })
    ));
    std::fs::remove_dir_all(tmp).unwrap();
}

/// Generate every design as a bare-metal and as an RTOS crate and build the library and examples. Skipped (with a note)
/// if the target is not installed.
#[test]
fn every_design_builds_for_the_target() {
    build_all(Platform::BareMetal);
}

#[test]
fn every_design_builds_with_the_kernel() {
    build_all(Platform::Rtos);
}

fn build_all(platform: Platform) {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("aarch64-unknown-none"))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipping: `rustup target add aarch64-unknown-none` to run this test");
        return;
    }
    for path in FIXTURES {
        let name = path.rsplit('/').next().unwrap();
        let tmp = std::env::temp_dir().join(format!(
            "hrx-psu-{name}-{platform:?}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        let o = Options {
            crate_name: Some("design_1".into()),
            ..opts(platform, root().to_str().unwrap())
        };
        generate(&fixture(path), &o)
            .unwrap()
            .write_to(&tmp)
            .unwrap();
        // Standalone package, with a target directory of its own (see the Zynq-7000 test).
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
                root().join(format!("target/gen-test-psu/{platform:?}-{name}")),
            )
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "{name}:\n{text}");
        assert!(
            !text.contains("warning"),
            "{name}: generated crate has warnings:\n{text}"
        );
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
