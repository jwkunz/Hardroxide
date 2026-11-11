//! Generate a Rust HAL crate from an [`Xsa`]. Output is a pure function of
//! the inputs: no timestamps, sorted file map, stable naming.

mod names;
mod render;

use hardroxide_hwh::{BitError, BitFile, Xsa};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error(transparent)]
    Bit(#[from] BitError),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Cargo package name of the generated crate. Defaults to the block
    /// design name.
    pub crate_name: Option<String>,
    /// Root of a Hardroxide checkout. When set, the generated crate depends
    /// on the runtime crates by path; otherwise by crates.io version.
    pub hardroxide_path: Option<PathBuf>,
}

/// The generated crate: relative path -> contents, in sorted order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedCrate {
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl GeneratedCrate {
    pub fn write_to(&self, dir: &Path) -> Result<(), GenError> {
        for (rel, data) in &self.files {
            let p = dir.join(rel);
            let io = |source| GenError::Io { path: p.clone(), source };
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            std::fs::write(&p, data).map_err(io)?;
        }
        Ok(())
    }
}

pub fn generate(xsa: &Xsa, opts: &Options) -> Result<GeneratedCrate, GenError> {
    let bit = BitFile::parse(&xsa.bit)?;
    let bin = bit.to_bin_swapped()?;
    let model = render::Model::build(&xsa.design);
    let crate_name = opts
        .crate_name
        .clone()
        .unwrap_or_else(|| names::snake(&xsa.design.name));

    let mut files = BTreeMap::new();
    files.insert("Cargo.toml".into(), render::cargo_toml(&crate_name, opts).into_bytes());
    files.insert("src/pac.rs".into(), render::pac_rs(&model).into_bytes());
    files.insert(
        "src/lib.rs".into(),
        render::lib_rs(&model, &xsa.design, &bin).into_bytes(),
    );
    if let Some(ex) = render::gpio_example(&model, &crate_name) {
        files.insert("examples/gpio_smoke.rs".into(), ex.into_bytes());
    }
    files.insert("bitstream/design.bin".into(), bin);
    Ok(GeneratedCrate { files })
}
