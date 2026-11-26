//! Linux crates for Zynq UltraScale+: what they contain, that every RFSoC 4x2 design's crate and examples type-check for
//! AArch64 Linux, and that loading the base overlay's crate (on the host, with mock registers and a fake `fpga_manager`)
//! sets the ports and clocks the design asks for. Nothing here has run on a board.

use hardroxide_gen::update::{Outcome, update};
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

fn opts() -> Options {
    Options {
        hardroxide_path: Some(root()),
        crate_name: Some("design_1".into()),
        ..Default::default()
    }
}

fn text<'a>(g: &'a hardroxide_gen::GeneratedCrate, file: &str) -> std::borrow::Cow<'a, str> {
    String::from_utf8_lossy(&g.files[Path::new(file)])
}

#[test]
fn the_crate_carries_this_familys_bitstream_ports_and_clocks() {
    let xsa = fixture(BASE);
    let g = generate(&xsa, &opts()).unwrap();
    assert_eq!(g, generate(&xsa, &opts()).unwrap());
    // The bitstream's words byte-swapped and nothing added (what `bootgen -arch zynqmp` writes).
    let bit = hardroxide_hwh::BitFile::parse(&xsa.bit).unwrap();
    let bin = &g.files[Path::new("bitstream/design.bin")];
    assert_eq!(bin.len(), bit.payload.len());
    let swapped: Vec<u8> = bit.payload[..256]
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|w| [w[3], w[2], w[1], w[0]])
        .collect();
    assert_eq!(bin[..256], swapped[..]);
    assert_eq!(
        text(&g, "bitstream/ports.cfg"),
        "m0=128\nm1=128\nm2=32\ns2=128\ns4=128\ns6=128\n"
    );
    assert_eq!(
        text(&g, "bitstream/load.cfg"),
        "ps_clk_hz=33333330\nfclk0=99999985\nfclk1=299999939\n"
    );
    let lib = text(&g, "src/lib.rs");
    assert!(lib.contains("hardroxide_fpga::zynqmp::load_bitstream"));
    assert!(!lib.contains("slcr"));
    // The base overlay's one line to the processor is the one the image's device tree already serves.
    assert!(text(&g, "src/pac.rs").contains("dt_node: \"fabric\""));
    assert!(!g.files.contains_key(Path::new("interrupts.dtso")));
    for file in ["examples/board_check.rs", "examples/irq_probe.rs"] {
        let example = text(&g, file);
        assert!(
            !example.contains("slcr") && !example.contains("IRQ_F2P"),
            "{file}"
        );
    }
    // From crates.io the crates are the 3.x ones.
    let published = generate(&xsa, &Options::default()).unwrap();
    let manifest = text(&published, "Cargo.toml");
    assert!(manifest.contains("hardroxide-fpga = \"3\""));
    assert!(manifest.contains("hardroxide-hwh = { version = \"3\", optional = true }"));
    assert!(!manifest.contains("\"2\"\n") || manifest.contains("thiserror = \"2\""));
}

#[test]
fn the_overlay_describes_the_other_lines_for_this_familys_tree() {
    let g = generate(&fixture("rfsoc4x2/rfsoc4x2_dma"), &opts()).unwrap();
    let overlay = text(&g, "interrupts.dtso");
    // pl_ps_irq0[0] is the image's; [1] and both of the second port's get nodes under the GIC, with 64-bit registers.
    assert!(overlay.contains(
        "axi_dma_0.mm2s_introut: SPI cell 89, served by the image's own `fabric` UIO node"
    ));
    assert!(overlay.contains("interrupt-parent = <&gic>;"));
    for cell in [90, 104, 105] {
        assert!(
            overlay.contains(&format!("interrupts = <0 {cell} 4>;")),
            "{cell}:\n{overlay}"
        );
    }
    assert!(
        overlay.contains("reg = <0x0 0xa0000000 0x0 0x10000>;")
            || overlay.contains("reg = <0x0 0xa0010000 0x0 0x10000>;")
    );
    assert!(!overlay.contains("&intc"));
}

#[test]
fn update_replaces_the_bitstream_files_of_an_unchanged_interface() {
    let tmp = std::env::temp_dir().join(format!("hrx-lpsu-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let mut xsa = fixture(BASE);
    generate(&xsa, &opts()).unwrap().write_to(&tmp).unwrap();
    assert_eq!(update(&tmp, &xsa, false).unwrap(), Outcome::Unchanged);
    // Another port width alone is a bitstream-side change.
    let ps = xsa.design.instances.iter_mut().find(|i| i.is_ps).unwrap();
    ps.parameters
        .insert("PSU__SAXIGP2__DATA_WIDTH".into(), "64".into());
    assert!(matches!(
        update(&tmp, &xsa, false).unwrap(),
        Outcome::BitstreamReplaced { .. }
    ));
    let ports = std::fs::read_to_string(tmp.join("bitstream/ports.cfg")).unwrap();
    assert!(ports.contains("s2=64"), "{ports}");
    std::fs::remove_dir_all(tmp).unwrap();
}

/// The overlay compiles without warnings and merges onto a tree shaped like the image's: the nodes arrive under the
/// root, wired to the GIC. Skipped (with a note) without `dtc` and `fdtoverlay` (or `DTC` and `FDTOVERLAY`).
#[test]
fn the_overlay_applies_to_a_tree_like_the_images() {
    let tool = |env: &str, name: &str| {
        std::env::var_os(env).map(PathBuf::from).or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|d| d.join(name))
                .find(|p| p.is_file())
        })
    };
    let (Some(dtc), Some(fdtoverlay)) = (tool("DTC", "dtc"), tool("FDTOVERLAY", "fdtoverlay"))
    else {
        eprintln!("skipping: dtc/fdtoverlay not found");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("hrx-lpsu-dtso-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let g = generate(&fixture("rfsoc4x2/rfsoc4x2_dma"), &opts()).unwrap();
    std::fs::write(tmp.join("ov.dtso"), &g.files[Path::new("interrupts.dtso")]).unwrap();
    std::fs::write(
        tmp.join("base.dts"),
        "/dts-v1/;\n/ {\n #address-cells = <2>;\n #size-cells = <2>;\n gic: interrupt-controller@f9010000 {\n  compatible = \"arm,gic-400\";\n  #interrupt-cells = <3>;\n  interrupt-controller;\n  #address-cells = <0>;\n  reg = <0x0 0xf9010000 0x0 0x10000>;\n };\n};\n",
    )
    .unwrap();
    let run = |program: &Path, args: &[&str]| {
        let out = Command::new(program)
            .args(args)
            .current_dir(&tmp)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?}: {said}");
        // Two interrupts of one block share its address, so their nodes do too (as on the Zynq-7000).
        assert!(
            !said
                .lines()
                .any(|l| l.contains("Warning") && !l.contains("unique_unit_address")),
            "{args:?}: {said}"
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    run(
        &dtc,
        &["-@", "-I", "dts", "-O", "dtb", "-o", "base.dtb", "base.dts"],
    );
    run(
        &dtc,
        &["-@", "-I", "dts", "-O", "dtb", "-o", "ov.dtbo", "ov.dtso"],
    );
    run(
        &fdtoverlay,
        &["-i", "base.dtb", "-o", "merged.dtb", "ov.dtbo"],
    );
    let merged = run(&dtc, &["-q", "-I", "dtb", "-O", "dts", "merged.dtb"]);
    for want in [
        "axi_dma_0_s2mm_introut@a0000000",
        "axi_dma_1_mm2s_introut@a0010000",
        "interrupts = <0x00 0x5a 0x04>",
        "interrupts = <0x00 0x68 0x04>",
        "reg = <0x00 0xa0010000 0x00 0x10000>",
    ] {
        assert!(merged.contains(want), "no `{want}` in\n{merged}");
    }
    assert!(
        !merged.contains("axi_dma_0_mm2s_introut"),
        "line 89 is the image's"
    );
    std::fs::remove_dir_all(tmp).unwrap();
}

/// Run `cargo` on a generated crate; its output must carry no warnings.
fn cargo(dir: &Path, target_dir: &str, args: &[&str]) {
    let mut manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    if !manifest.contains("[workspace]") {
        manifest.push_str("\n[workspace]\n");
        std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    }
    let out = Command::new(env!("CARGO"))
        .args(args)
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root().join("target").join(target_dir))
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
}

#[test]
fn every_design_checks_for_aarch64_linux() {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("aarch64-unknown-linux-gnu"))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipping: `rustup target add aarch64-unknown-linux-gnu` to run this test");
        return;
    }
    for path in FIXTURES {
        let name = path.rsplit('/').next().unwrap();
        let tmp = std::env::temp_dir().join(format!("hrx-lpsu-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        generate(&fixture(path), &opts())
            .unwrap()
            .write_to(&tmp)
            .unwrap();
        // Checking needs no cross linker.
        cargo(
            &tmp,
            &format!("gen-test-linux-psu/{name}"),
            &[
                "check",
                "--all-targets",
                "--features",
                "dev-bitstream",
                "--target",
                "aarch64-unknown-linux-gnu",
            ],
        );
        std::fs::remove_dir_all(tmp).unwrap();
    }
}

#[test]
fn loading_the_base_overlay_sets_its_ports_and_clocks() {
    let tmp = std::env::temp_dir().join(format!("hrx-lpsu-mock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    generate(&fixture(BASE), &opts())
        .unwrap()
        .write_to(&tmp)
        .unwrap();
    std::fs::create_dir_all(tmp.join("tests")).unwrap();
    std::fs::write(
        tmp.join("tests/mock.rs"),
        include_str!("data/generated_zynqmp_test.rs"),
    )
    .unwrap();
    cargo(&tmp, "gen-test-linux-psu/mock", &["test"]);
    std::fs::remove_dir_all(tmp).unwrap();
}
