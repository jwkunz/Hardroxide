use hardroxide_gen::{Options, generate};
use hardroxide_hwh::Xsa;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture_named(n: &str) -> Xsa {
    Xsa::open(root().join(format!("fixtures/{n}.xsa"))).unwrap()
}

fn fixture() -> Xsa {
    fixture_named("pynq_z2_gpio")
}

const FIXTURES: &[&str] = &[
    "pynq_z2_gpio",
    "pynq_z2_hls_vadd",
    "pynq_z2_dma_loopback",
    "pynq_z2_peripherals",
    "pynq_z2_intc",
    "pynq_z2_hls_scale",
];

#[test]
fn deterministic() {
    let o = Options::default();
    assert_eq!(
        generate(&fixture(), &o).unwrap(),
        generate(&fixture(), &o).unwrap()
    );
}

/// Golden snapshot of the generated sources. Regenerate with
/// `UPDATE_GOLDEN=1 cargo test -p hardroxide-gen`.
#[test]
fn golden_snapshot() {
    let opts = Options {
        hardroxide_path: Some("/hardroxide".into()),
        ..Default::default()
    };
    for name in FIXTURES {
        let g = generate(&fixture_named(name), &opts).unwrap();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name);
        for (rel, data) in &g.files {
            if rel.extension().is_some_and(|e| e == "bin") {
                continue; // big; covered by bitstream/design.sha256
            }
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

/// Generate each crate, compile it, and run a mock-MMIO test against it.
fn build_and_test(fixture: &str, crate_name: &str, test_src: &str) {
    let tmp = std::env::temp_dir().join(format!("hrx-gen-{crate_name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let opts = Options {
        hardroxide_path: Some(root()),
        crate_name: Some(crate_name.into()),
    };
    generate(&fixture_named(fixture), &opts)
        .unwrap()
        .write_to(&tmp)
        .unwrap();
    std::fs::create_dir_all(tmp.join("tests")).unwrap();
    let fixture_path = root().join(format!("fixtures/{fixture}.xsa"));
    std::fs::write(
        tmp.join("tests/mock.rs"),
        test_src.replace("__FIXTURE__", fixture_path.to_str().unwrap()),
    )
    .unwrap();
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
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(
        !text.contains("warning"),
        "generated crate has warnings:\n{text}"
    );
    std::fs::remove_dir_all(tmp).unwrap();
}

#[test]
fn generated_gpio_crate_builds_and_works_with_mock() {
    build_and_test(
        "pynq_z2_gpio",
        "pynq_z2_gpio_test",
        include_str!("data/generated_mock_test.rs"),
    );
}

#[test]
fn generated_dma_crate_builds_and_works_with_mock() {
    build_and_test(
        "pynq_z2_dma_loopback",
        "pynq_z2_dma_test",
        include_str!("data/generated_dma_test.rs"),
    );
}

#[test]
fn generated_peripherals_crate_builds_and_works_with_mock() {
    build_and_test(
        "pynq_z2_peripherals",
        "pynq_z2_periph_test",
        include_str!("data/generated_periph_test.rs"),
    );
}

#[test]
fn generated_hls_crate_builds_and_works_with_mock() {
    build_and_test(
        "pynq_z2_hls_vadd",
        "pynq_z2_hls_test",
        include_str!("data/generated_hls_test.rs"),
    );
}

#[test]
fn generated_hls_scale_crate_builds_and_works_with_mock() {
    build_and_test(
        "pynq_z2_hls_scale",
        "pynq_z2_scale_test",
        include_str!("data/generated_scale_test.rs"),
    );
}

#[test]
fn generated_intc_crate_builds_and_works_with_mock() {
    build_and_test(
        "pynq_z2_intc",
        "pynq_z2_intc_test",
        include_str!("data/generated_intc_test.rs"),
    );
}

fn fresh_crate(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hrx-upd-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    generate(&fixture(), &Options::default())
        .unwrap()
        .write_to(&dir)
        .unwrap();
    dir
}

fn snapshot(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut m = std::collections::BTreeMap::new();
    fn walk(base: &Path, d: &Path, m: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, m)
            } else {
                m.insert(
                    p.strip_prefix(base).unwrap().into(),
                    std::fs::read(&p).unwrap(),
                );
            }
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
    assert!(matches!(
        update(&dir, &x, false).unwrap(),
        Outcome::BitstreamReplaced { .. }
    ));
    let after = snapshot(&dir);
    let changed: Vec<_> = after
        .iter()
        .filter(|(k, v)| before.get(*k) != Some(v))
        .map(|(k, _)| k.to_str().unwrap())
        .collect();
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
    let Outcome::InterfaceChanged { diff } = update(&dir, &x, false).unwrap() else {
        panic!()
    };
    assert!(
        diff.iter()
            .any(|d| matches!(d, DiffLine::Added(l) if l.contains("0x43000000")))
    );
    assert_eq!(snapshot(&dir), before, "no files touched without --regen");
    assert!(matches!(
        update(&dir, &x, true).unwrap(),
        Outcome::Regenerated { .. }
    ));
    let pac = std::fs::read_to_string(dir.join("src/pac.rs")).unwrap();
    assert!(pac.contains("0x43000000"));
    assert_eq!(
        std::fs::read_to_string(dir.join("Cargo.toml")).unwrap(),
        String::from_utf8(before[Path::new("Cargo.toml")].clone()).unwrap()
    );
    assert_eq!(update(&dir, &x, false).unwrap(), Outcome::Unchanged);
    std::fs::remove_dir_all(dir).unwrap();
}

fn tool(env: &str, name: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env) {
        return Some(p.into());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// The generated overlay must apply cleanly to a Zynq-7000-style base tree
/// with a labelled `intc`, and the SPI cells must survive the merge.
/// Needs `dtc` and `fdtoverlay` (env `DTC` / `FDTOVERLAY` or PATH); skipped
/// when they are absent (CI installs `device-tree-compiler`).
#[test]
fn interrupts_overlay_applies_with_fdtoverlay() {
    let (Some(dtc), Some(fdt)) = (tool("DTC", "dtc"), tool("FDTOVERLAY", "fdtoverlay")) else {
        eprintln!("skipping: dtc/fdtoverlay not found");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("hrx-dtso-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let g = generate(&fixture_named("pynq_z2_dma_loopback"), &Options::default()).unwrap();
    std::fs::write(tmp.join("ov.dts"), &g.files[Path::new("interrupts.dtso")]).unwrap();
    std::fs::write(
        tmp.join("base.dts"),
        "/dts-v1/;\n/ {\n #address-cells = <1>;\n #size-cells = <1>;\n intc: interrupt-controller@f8f01000 {\n  compatible = \"arm,cortex-a9-gic\";\n  #interrupt-cells = <3>;\n  interrupt-controller;\n  reg = <0xf8f01000 0x1000 0xf8f00100 0x100>;\n };\n};\n",
    )
    .unwrap();
    let run = |cmd: &Path, args: &[&str]| {
        let o = Command::new(cmd)
            .args(args)
            .current_dir(&tmp)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{} {args:?}: {}",
            cmd.display(),
            String::from_utf8_lossy(&o.stderr)
        );
        o
    };
    run(
        &dtc,
        &["-@", "-I", "dts", "-O", "dtb", "-o", "base.dtb", "base.dts"],
    );
    run(
        &dtc,
        &["-@", "-I", "dts", "-O", "dtb", "-o", "ov.dtb", "ov.dts"],
    );
    run(&fdt, &["-i", "base.dtb", "-o", "merged.dtb", "ov.dtb"]);
    let dts = run(&dtc, &["-I", "dtb", "-O", "dts", "merged.dtb"]).stdout;
    let dts = String::from_utf8_lossy(&dts);
    for want in [
        "axi_dma_0_mm2s_introut@40400000",
        "axi_dma_0_s2mm_introut@40400000",
        "interrupts = <0x00 0x1d 0x04>",
        "interrupts = <0x00 0x1e 0x04>",
    ] {
        assert!(dts.contains(want), "merged tree lacks `{want}`:\n{dts}");
    }
    std::fs::remove_dir_all(tmp).unwrap();
}
