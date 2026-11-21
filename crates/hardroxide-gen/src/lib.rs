//! Generate a Rust HAL crate from an [`Xsa`]. Output is a pure function of
//! the inputs: no timestamps, sorted file map, stable naming.

mod baremetal;
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
    #[error(
        "unsupported architecture `{arch}` (device `{device}`): only Zynq-7000 (`zynq`) is supported; \
         the bitstream conversion, clock programming and interrupt numbering are specific to it. \
         `hardroxide inspect` still works on this design"
    )]
    UnsupportedArch { arch: String, device: String },
    #[error(
        "the `{0}` platform is not available yet (see docs/decisions/0003-bare-metal-and-rtos-platforms.md)"
    )]
    PlatformUnavailable(&'static str),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The kind of crate to generate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Platform {
    /// Linux userspace on the PS (the default): embeds the bitstream, programs the PL
    /// through the FPGA manager, maps IP through `/dev/mem`.
    #[default]
    Linux,
    /// A `no_std` program for core 0 with no operating system, built on `hardroxide-ps7`.
    BareMetal,
    /// Bare metal plus a FreeRTOS-style real-time kernel (not available yet).
    Rtos,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::BareMetal => "baremetal",
            Platform::Rtos => "rtos",
        }
    }
}

impl std::str::FromStr for Platform {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "linux" => Ok(Platform::Linux),
            "baremetal" => Ok(Platform::BareMetal),
            "rtos" => Ok(Platform::Rtos),
            other => Err(format!(
                "unknown platform `{other}` (expected linux, baremetal or rtos)"
            )),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Which kind of crate to generate. Defaults to [`Platform::Linux`].
    pub platform: Platform,
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

/// What a host-visible instance becomes in the generated crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A curated driver from `hardroxide-ip` (its type path).
    Driver(String),
    /// A typed wrapper generated from an HLS kernel's register map.
    Hls(String),
    /// No driver: a raw register window (`B::Region`) plus register constants.
    Raw,
}

/// One instance of the design as seen from the PS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSummary {
    pub name: String,
    /// The field name on the generated `Design`.
    pub field: String,
    pub vlnv: String,
    pub base: u64,
    pub size: u64,
    pub kind: Kind,
    /// One line per connected interrupt output.
    pub interrupts: Vec<String>,
}

/// Summary of what [`generate`] makes of a design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Instances software on the PS can reach, in address order.
    pub instances: Vec<InstanceSummary>,
    /// Instances that exist but are not reachable from the PS (they belong to
    /// another master's private address map, e.g. a MicroBlaze), or have no
    /// address at all. They get no field.
    pub not_host_visible: Vec<String>,
}

/// Only Zynq-7000 is supported; anything else would produce a crate that
/// programs the wrong registers.
pub(crate) fn check_arch(d: &hardroxide_hwh::Design) -> Result<(), GenError> {
    if d.arch.eq_ignore_ascii_case("zynq") {
        Ok(())
    } else {
        Err(GenError::UnsupportedArch {
            arch: d.arch.clone(),
            device: d.device.clone(),
        })
    }
}

/// Describe, instance by instance, what `generate` would produce.
pub fn summarize(design: &hardroxide_hwh::Design) -> Summary {
    let model = render::Model::build(design);
    let instances = model
        .insts
        .iter()
        .map(|i| InstanceSummary {
            name: i.inst.name.clone(),
            field: i.ident.clone(),
            vlnv: i.inst.vlnv.clone(),
            base: i.ranges[0].base,
            size: i.ranges[0].size(),
            kind: match (&i.generated, &i.driver) {
                (Some(t), _) => Kind::Hls(format!("crate::ips::{t}")),
                (None, Some(d)) => Kind::Driver(d.clone()),
                (None, None) => Kind::Raw,
            },
            interrupts: render::irq_lines(i, design)
                .into_iter()
                .map(|l| {
                    let to = match (&l.via_intc, l.f2p_bit) {
                        (Some((c, b)), _) => format!("input {b} of {c}"),
                        (None, Some(b)) => match l.gic_spi {
                            Some(spi) => format!("IRQ_F2P[{b}] (GIC SPI {spi})"),
                            None => format!("IRQ_F2P[{b}]"),
                        },
                        (None, None) => "not routed to the PS (no UIO device)".into(),
                    };
                    format!("{} -> {to}", l.port)
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let mut instances = instances;
    instances.sort_by_key(|i| i.base);
    let mut not_host_visible: Vec<String> = design
        .instances
        .iter()
        .filter(|i| !i.is_ps && design.ranges_of(&i.name).next().is_none())
        .filter(|i| design.memory_ranges.iter().any(|r| r.instance == i.name))
        .map(|i| i.name.clone())
        .collect();
    not_host_visible.sort();
    Summary {
        instances,
        not_host_visible,
    }
}

fn generate_baremetal(xsa: &Xsa, opts: &Options) -> Result<GeneratedCrate, GenError> {
    let model = render::Model::build(&xsa.design);
    let d = &xsa.design;
    let crate_name = opts
        .crate_name
        .clone()
        .unwrap_or_else(|| names::snake(&d.name));
    let mut files: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        files.insert(k.into(), v.into_bytes());
    };
    put("Cargo.toml", baremetal::cargo_toml(&crate_name, opts));
    put(".cargo/config.toml", baremetal::cargo_config());
    put("build.rs", baremetal::build_rs());
    put("link.ld", baremetal::link_ld());
    put("src/pac.rs", render::pac_rs(&model, d));
    put("src/lib.rs", baremetal::lib_rs(&model, d));
    if let Some(ips) = render::ips_rs(&model) {
        put("src/ips.rs", ips);
    }
    put(
        "examples/board_check.rs",
        baremetal::board_check_example(&model, d, &crate_name),
    );
    for (name, ex) in [
        ("gpio_smoke", baremetal::gpio_example(&model, &crate_name)),
        ("timer_delay", baremetal::timer_example(&model, &crate_name)),
        (
            "timer_irq",
            baremetal::timer_irq_example(&model, d, &crate_name),
        ),
        ("bram_test", baremetal::bram_example(&model, &crate_name)),
        ("hls_smoke", baremetal::hls_example(&model, &crate_name)),
    ] {
        if let Some(ex) = ex {
            put(&format!("examples/{name}.rs"), ex);
        }
    }
    put("interface.txt", d.interface_text());
    put("interface.sha256", d.interface_hash());
    Ok(GeneratedCrate { files })
}

pub fn generate(xsa: &Xsa, opts: &Options) -> Result<GeneratedCrate, GenError> {
    check_arch(&xsa.design)?;
    match opts.platform {
        Platform::Linux => {}
        Platform::BareMetal => return generate_baremetal(xsa, opts),
        Platform::Rtos => return Err(GenError::PlatformUnavailable("rtos")),
    }
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
        ("fifo_loopback", render::fifo_example(&model, &crate_name)),
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
