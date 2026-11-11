//! Generate a Rust HAL crate from an [`Xsa`]. Output is a pure function of
//! the inputs: no timestamps, sorted file map, stable naming.

mod names;
mod render;

use hardroxide_fpga::{LoadConfig, sha256_hex};
use hardroxide_hwh::{BitError, Xsa};

pub mod update;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error(transparent)]
    Bit(#[from] BitError),
    #[error(transparent)]
    Xsa(#[from] hardroxide_hwh::Error),
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
            let io = |source| GenError::Io {
                path: p.clone(),
                source,
            };
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            std::fs::write(&p, data).map_err(io)?;
        }
        Ok(())
    }
}

/// The files that change when only the bitstream changes.
pub(crate) fn bitstream_files(xsa: &Xsa) -> Result<Vec<(PathBuf, Vec<u8>)>, GenError> {
    let bin = xsa.bin()?;
    let cfg = LoadConfig {
        ps_clk_hz: xsa.design.ps_clk_hz,
        fclks: xsa.design.enabled_fclks(),
    };
    Ok(vec![
        (
            "bitstream/design.sha256".into(),
            sha256_hex(&bin).into_bytes(),
        ),
        ("bitstream/load.cfg".into(), cfg.to_text().into_bytes()),
        ("bitstream/design.bin".into(), bin),
    ])
}

pub fn generate(xsa: &Xsa, opts: &Options) -> Result<GeneratedCrate, GenError> {
    let model = render::Model::build(&xsa.design);
    let crate_name = opts
        .crate_name
        .clone()
        .unwrap_or_else(|| names::snake(&xsa.design.name));

    let mut files = BTreeMap::new();
    files.insert(
        "Cargo.toml".into(),
        render::cargo_toml(&crate_name, opts).into_bytes(),
    );
    files.insert(
        "src/pac.rs".into(),
        render::pac_rs(&model, &xsa.design).into_bytes(),
    );
    files.insert(
        "src/lib.rs".into(),
        render::lib_rs(&model, &xsa.design).into_bytes(),
    );
    if let Some(ex) = render::gpio_example(&model, &crate_name) {
        files.insert("examples/gpio_smoke.rs".into(), ex.into_bytes());
    }
    for (name, ex) in [
        ("timer_delay", render::timer_example(&model, &crate_name)),
        ("spi_loopback", render::spi_example(&model, &crate_name)),
        ("i2c_scan", render::iic_example(&model, &crate_name)),
        ("bram_test", render::bram_example(&model, &crate_name)),
        ("uart_hello", render::uart_example(&model, &crate_name)),
    ] {
        if let Some(ex) = ex {
            files.insert(format!("examples/{name}.rs").into(), ex.into_bytes());
        }
    }
    files.insert(
        "examples/board_check.rs".into(),
        render::board_check_example(&model, &xsa.design, &crate_name).into_bytes(),
    );
    if let Some(ex) = render::dma_example(&model, &crate_name) {
        files.insert("examples/dma_loopback.rs".into(), ex.into_bytes());
    }
    if let Some(dtso) = render::interrupts_dtso(&model, &xsa.design) {
        files.insert("interrupts.dtso".into(), dtso.into_bytes());
    }
    if let Some(ex) = render::irq_example(&model, &xsa.design, &crate_name) {
        files.insert("examples/irq_probe.rs".into(), ex.into_bytes());
    }
    if let Some(ips) = render::ips_rs(&model) {
        files.insert("src/ips.rs".into(), ips.into_bytes());
    }
    if let Some(ex) = render::hls_example(&model, &crate_name) {
        files.insert("examples/hls_smoke.rs".into(), ex.into_bytes());
    }
    files.insert(
        "interface.txt".into(),
        xsa.design.interface_text().into_bytes(),
    );
    files.insert(
        "interface.sha256".into(),
        xsa.design.interface_hash().into_bytes(),
    );
    for (k, v) in bitstream_files(xsa)? {
        files.insert(k, v);
    }
    Ok(GeneratedCrate { files })
}
