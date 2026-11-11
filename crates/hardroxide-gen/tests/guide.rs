//! Compiles the Rust examples in the user guide against freshly generated
//! crates. A block tagged `rust,ignore,fixture=<name>` is wrapped in a `main`
//! (unless it already defines one), with `# ` hidden lines as setup, and
//! `my_design` replaced by the generated crate's name. Type-checked only.

use hardroxide_gen::{Options, generate};
use hardroxide_hwh::Xsa;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// (fixture, source) for every tagged block in the guide.
fn guide_blocks() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(root().join("docs/guide"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    for f in files {
        let md = std::fs::read_to_string(&f).unwrap();
        let mut cur: Option<(String, Vec<String>)> = None;
        for line in md.lines() {
            if let Some((fx, body)) = &mut cur {
                if line.starts_with("```") {
                    out.push((fx.clone(), body.join("\n")));
                    cur = None;
                } else {
                    let l = line
                        .strip_prefix("# ")
                        .or_else(|| (line == "#").then_some(""))
                        .unwrap_or(line);
                    body.push(l.to_string());
                }
            } else if let Some(fx) = line.strip_prefix("```rust,ignore,fixture=") {
                cur = Some((fx.trim().to_string(), vec![]));
            }
        }
    }
    out
}

#[test]
fn guide_examples_compile() {
    let blocks = guide_blocks();
    assert!(
        blocks.len() >= 5,
        "found only {} guide examples",
        blocks.len()
    );
    let mut by_fixture: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (fx, src) in blocks {
        by_fixture.entry(fx).or_default().push(src);
    }
    for (fx, sources) in by_fixture {
        let name = format!("my_design_{fx}");
        let tmp = std::env::temp_dir().join(format!("hrx-guide-{fx}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let xsa = Xsa::open(root().join(format!("fixtures/{fx}.xsa"))).unwrap();
        let opts = Options {
            hardroxide_path: Some(root()),
            crate_name: Some(name.clone()),
        };
        generate(&xsa, &opts).unwrap().write_to(&tmp).unwrap();
        for (i, src) in sources.iter().enumerate() {
            let src = src.replace("my_design", &name);
            let body = if src.contains("fn main") {
                src
            } else {
                // Hoist `use` lines; run the rest inside main.
                let (uses, rest): (Vec<_>, Vec<_>) =
                    src.lines().partition(|l| l.starts_with("use "));
                format!(
                    "{}\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n{}\nOk(())\n}}\n",
                    uses.join("\n"),
                    rest.join("\n")
                )
            };
            std::fs::write(
                tmp.join(format!("examples/guide_{i}.rs")),
                format!("#![allow(unused, unreachable_code)]\n{body}"),
            )
            .unwrap();
        }
        let mut manifest = std::fs::read_to_string(tmp.join("Cargo.toml")).unwrap();
        manifest.push_str("\n[workspace]\n");
        std::fs::write(tmp.join("Cargo.toml"), manifest).unwrap();
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
        assert!(
            out.status.success(),
            "guide examples for {fx} do not compile:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
