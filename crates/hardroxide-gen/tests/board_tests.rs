//! Compiles every on-board test program in `fixtures/board_tests/<dir>/` (the
//! programs the tutorials show) against a freshly generated crate for the
//! fixture that directory is named after. Type-checked only; running them needs
//! a board. Keeps the tutorials' code from drifting from the API.

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

/// (directory under fixtures/board_tests, XSA relative to fixtures/, crate name)
const SETS: &[(&str, &str, &str)] = &[
    ("gpio", "pynq_z2_gpio.xsa", "design_1"),
    ("peripherals", "pynq_z2_peripherals.xsa", "design_1"),
    ("hls_vadd", "pynq_z2_hls_vadd.xsa", "design_1"),
    ("hls_scale", "pynq_z2_hls_scale.xsa", "design_1"),
    ("dma_loopback", "pynq_z2_dma_loopback.xsa", "design_1"),
    ("intc", "pynq_z2_intc.xsa", "design_1"),
    ("mem_uart", "pynq_z2_mem_uart.xsa", "design_1"),
    ("fifo", "pynq_z2_fifo.xsa", "design_1"),
    ("loopback", "pynq_z2_loopback.xsa", "design_1"),
    ("base", "real/pynq_base_overlay.xsa", "base"),
];

#[test]
fn board_test_programs_compile() {
    for (dir, xsa, name) in SETS {
        let src_dir = root().join("fixtures/board_tests").join(dir);
        let programs: Vec<_> = std::fs::read_dir(&src_dir)
            .unwrap_or_else(|e| panic!("{}: {e}", src_dir.display()))
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .collect();
        assert!(!programs.is_empty(), "no programs in {}", src_dir.display());

        let tmp = std::env::temp_dir().join(format!("hrx-bt-{dir}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let x = Xsa::open(root().join("fixtures").join(xsa)).unwrap();
        let opts = Options {
            hardroxide_path: Some(root()),
            crate_name: Some((*name).to_string()),
        };
        generate(&x, &opts).unwrap().write_to(&tmp).unwrap();
        for p in &programs {
            let text = std::fs::read_to_string(p).unwrap();
            if text.contains("wait_async") {
                // These need the optional async API of hardroxide-irq.
                let cargo = tmp.join("Cargo.toml");
                let t = std::fs::read_to_string(&cargo).unwrap();
                let t = t
                    .lines()
                    .map(|l| {
                        if l.starts_with("hardroxide-irq = { path") {
                            l.replace(" }", ", features = [\"async\"] }")
                        } else {
                            l.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                std::fs::write(cargo, t).unwrap();
            }
            std::fs::copy(p, tmp.join("examples").join(p.file_name().unwrap())).unwrap();
        }
        let out = Command::new(env!("CARGO"))
            .args([
                "check",
                "--examples",
                "--features",
                "dev-bitstream",
                "--manifest-path",
            ])
            .arg(tmp.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", root().join("target/gen-test"))
            .output()
            .unwrap();
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success(),
            "board test programs for `{dir}` do not compile:\n{text}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
