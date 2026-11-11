//! `hardroxide update`: refresh a generated crate from a new XSA.
//!
//! * Same interface hash: only the bitstream files are replaced, no Rust
//!   source changes.
//! * Different interface: report a diff and require `regen`.

use crate::{GenError, Options, bitstream_files, generate};
use hardroxide_hwh::Xsa;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Bitstream (and load config) replaced; sources untouched.
    BitstreamReplaced {
        old_sha256: String,
        new_sha256: String,
    },
    /// Nothing changed at all.
    Unchanged,
    /// Interface differs and `regen` was not requested.
    InterfaceChanged { diff: Vec<DiffLine> },
    /// Interface differed; the crate was regenerated.
    Regenerated { diff: Vec<DiffLine> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Removed(String),
    Added(String),
}

impl std::fmt::Display for DiffLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffLine::Removed(l) => write!(f, "- {l}"),
            DiffLine::Added(l) => write!(f, "+ {l}"),
        }
    }
}

/// Line-set difference between two canonical interface texts.
pub fn diff_interface(old: &str, new: &str) -> Vec<DiffLine> {
    let o: BTreeSet<&str> = old.lines().collect();
    let n: BTreeSet<&str> = new.lines().collect();
    let mut out: Vec<_> = old
        .lines()
        .filter(|l| !n.contains(l))
        .map(|l| DiffLine::Removed(l.into()))
        .collect();
    out.extend(
        new.lines()
            .filter(|l| !o.contains(l))
            .map(|l| DiffLine::Added(l.into())),
    );
    out
}

fn read(dir: &Path, rel: &str) -> Result<String, GenError> {
    let p = dir.join(rel);
    std::fs::read_to_string(&p).map_err(|source| GenError::Io { path: p, source })
}

fn package_name(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|l| {
        l.trim()
            .strip_prefix("name = \"")?
            .strip_suffix('"')
            .map(String::from)
    })
}

pub fn update(crate_dir: &Path, xsa: &Xsa, regen: bool) -> Result<Outcome, GenError> {
    let old_text = read(crate_dir, "interface.txt")?;
    let new_text = xsa.design.interface_text();
    if old_text == new_text {
        let old_sha256 = read(crate_dir, "bitstream/design.sha256")?;
        let files = bitstream_files(xsa)?;
        let new_sha256 = String::from_utf8(files[0].1.clone()).unwrap();
        if old_sha256 == new_sha256
            && read(crate_dir, "bitstream/load.cfg")?.as_bytes() == files[1].1
        {
            return Ok(Outcome::Unchanged);
        }
        write_all(crate_dir, files)?;
        return Ok(Outcome::BitstreamReplaced {
            old_sha256,
            new_sha256,
        });
    }
    let diff = diff_interface(&old_text, &new_text);
    if !regen {
        return Ok(Outcome::InterfaceChanged { diff });
    }
    // Keep the existing Cargo.toml (name, dependency sources, user edits).
    let manifest = read(crate_dir, "Cargo.toml")?;
    let opts = Options {
        crate_name: package_name(&manifest),
        hardroxide_path: None,
    };
    let mut g = generate(xsa, &opts)?;
    g.files.remove(Path::new("Cargo.toml"));
    // Drop stale generated examples if the design no longer has a GPIO.
    if !g.files.contains_key(Path::new("examples/gpio_smoke.rs")) {
        let _ = std::fs::remove_file(crate_dir.join("examples/gpio_smoke.rs"));
    }
    g.write_to(crate_dir)?;
    Ok(Outcome::Regenerated { diff })
}

fn write_all(dir: &Path, files: Vec<(std::path::PathBuf, Vec<u8>)>) -> Result<(), GenError> {
    for (rel, data) in files {
        let p = dir.join(rel);
        std::fs::write(&p, data).map_err(|source| GenError::Io { path: p, source })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lines() {
        let d = diff_interface("a\nb\nc\n", "a\nc\nd\n");
        assert_eq!(
            d,
            [DiffLine::Removed("b".into()), DiffLine::Added("d".into())]
        );
        assert_eq!(
            package_name("[package]\nname = \"x_y\"\n"),
            Some("x_y".into())
        );
    }
}
