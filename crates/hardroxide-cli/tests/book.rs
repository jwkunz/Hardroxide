//! Keeps the book honest: reference signatures must exist in the source,
//! CLI help blocks must equal the real `--help`, and relative links resolve.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

fn files(dir: PathBuf, ext: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    walk(&dir, ext, &mut v);
    v.sort();
    v
}

/// Remove all whitespace and trailing commas before a closing bracket, so
/// rustfmt layout differences do not matter.
fn norm(s: &str) -> String {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    s.replace(",)", ")").replace(",]", "]").replace(",>", ">")
}

/// Lines of fenced blocks whose info string is exactly `info`.
fn blocks(md: &str, info: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut cur: Option<Vec<String>> = None;
    for line in md.lines() {
        let l = line.trim_start();
        match &mut cur {
            None if l.starts_with("```") && l[3..].trim() == info => cur = Some(vec![]),
            Some(_) if l.starts_with("```") => out.push(cur.take().unwrap()),
            Some(b) => b.push(line.to_string()),
            None => {}
        }
    }
    out
}

#[test]
fn reference_signatures_exist_in_source() {
    let mut src = String::new();
    for f in files(root().join("crates"), "rs") {
        if !f.components().any(|c| c.as_os_str() == "golden") {
            src.push_str(&norm(&std::fs::read_to_string(f).unwrap()));
        }
    }
    // The generator's template text is Rust source too, so it is covered above.
    let mut checked = 0;
    let mut missing = Vec::new();
    for f in files(root().join("docs/reference"), "md") {
        let md = std::fs::read_to_string(&f).unwrap();
        for block in blocks(&md, "rust,ignore") {
            for line in block {
                let t = line.trim();
                if t.is_empty() || t.starts_with("//") || t.chars().all(|c| "{}.".contains(c)) {
                    continue;
                }
                checked += 1;
                // Strip a trailing `{` or `;` the doc may carry.
                let t = t.trim_end_matches(['{', ';']);
                if !src.contains(&norm(t)) {
                    missing.push(format!("{}: {t}", f.file_name().unwrap().to_string_lossy()));
                }
            }
        }
    }
    assert!(
        checked > 100,
        "only {checked} signatures checked; is the book missing?"
    );
    assert!(
        missing.is_empty(),
        "signatures in the book not found in source:\n{}",
        missing.join("\n")
    );
}

#[test]
fn cli_help_blocks_match_real_output() {
    let md = std::fs::read_to_string(root().join("docs/reference/cli.md")).unwrap();
    let mut n = 0;
    for block in blocks(&md, "text") {
        let Some(cmd) = block.first().and_then(|l| l.strip_prefix("$ hardroxide ")) else {
            continue;
        };
        let args: Vec<&str> = cmd.split_whitespace().collect();
        let out = Command::new(env!("CARGO_BIN_EXE_hardroxide"))
            .args(&args)
            .output()
            .unwrap();
        // Compare modulo trailing spaces and runs of blank lines.
        let clean = |s: &str| {
            let mut out: Vec<String> = Vec::new();
            for l in s.lines().map(str::trim_end) {
                if !(l.is_empty() && out.last().is_none_or(|p| p.is_empty())) {
                    out.push(l.to_string());
                }
            }
            out.join("\n").trim().to_string()
        };
        let want = clean(&String::from_utf8_lossy(&out.stdout));
        let got = clean(&block[1..].join("\n"));
        assert_eq!(got, want, "help block for `hardroxide {cmd}` is stale");
        n += 1;
    }
    assert_eq!(
        n, 4,
        "expected help blocks for the CLI and its three commands"
    );
}

#[test]
fn relative_links_resolve() {
    let mut bad = Vec::new();
    for f in files(root().join("docs"), "md") {
        let md = std::fs::read_to_string(&f).unwrap();
        let mut in_code = false;
        for line in md.lines() {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
            }
            if in_code {
                continue;
            }
            let mut rest = line;
            while let Some(i) = rest.find("](") {
                rest = &rest[i + 2..];
                let Some(end) = rest.find(')') else { break };
                let target = &rest[..end];
                let path = target.split('#').next().unwrap();
                // `api/` is the rustdoc tree copied in at deploy time (pages.yml).
                if path.is_empty()
                    || path.contains("://")
                    || path.starts_with("mailto:")
                    || path.starts_with("api/")
                {
                    continue;
                }
                if !f.parent().unwrap().join(path).exists() {
                    bad.push(format!(
                        "{}: {target}",
                        f.strip_prefix(root()).unwrap().display()
                    ));
                }
            }
        }
    }
    assert!(bad.is_empty(), "broken relative links:\n{}", bad.join("\n"));
}
