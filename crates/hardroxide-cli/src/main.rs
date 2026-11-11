use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hardroxide_gen::{Options, generate};
use hardroxide_hwh::Xsa;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hardroxide", version, about = "Generate Rust HAL crates from Vivado XSA files")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a HAL crate from an XSA.
    Generate {
        xsa: PathBuf,
        /// Output directory (created if needed). Defaults to the crate name.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Cargo package name (default: block design name).
        #[arg(long)]
        name: Option<String>,
        /// Hardroxide checkout; depend on runtime crates by path.
        #[arg(long)]
        hardroxide_path: Option<PathBuf>,
    },
    /// Refresh an existing generated crate from a new XSA.
    ///
    /// Same interface: replaces only the bitstream files. Different
    /// interface: prints a diff and exits non-zero unless `--regen`.
    Update {
        xsa: PathBuf,
        /// The generated crate directory.
        #[arg(short, long, default_value = ".")]
        crate_dir: PathBuf,
        /// Regenerate sources if the interface changed.
        #[arg(long)]
        regen: bool,
    },
    /// Print a summary of an XSA: instances, addresses, clocks, hashes.
    Inspect { xsa: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Generate { xsa, out, name, hardroxide_path } => {
            let x = Xsa::open(&xsa).with_context(|| format!("reading {}", xsa.display()))?;
            let hardroxide_path = hardroxide_path
                .map(|p| p.canonicalize().with_context(|| format!("{}", p.display())))
                .transpose()?;
            let g = generate(&x, &Options { crate_name: name.clone(), hardroxide_path })?;
            let out = out.unwrap_or_else(|| PathBuf::from(name.unwrap_or_else(|| x.design.name.clone())));
            g.write_to(&out)?;
            println!("generated {} files in {}", g.files.len(), out.display());
        }
        Cmd::Update { xsa, crate_dir, regen } => {
            use hardroxide_gen::update::{Outcome, update};
            let x = Xsa::open(&xsa).with_context(|| format!("reading {}", xsa.display()))?;
            match update(&crate_dir, &x, regen)? {
                Outcome::Unchanged => println!("up to date"),
                Outcome::BitstreamReplaced { old_sha256, new_sha256 } => {
                    println!("interface unchanged; bitstream replaced\n  {old_sha256}\n  {new_sha256}");
                }
                Outcome::Regenerated { diff } => {
                    println!("interface changed; crate regenerated:");
                    diff.iter().for_each(|l| println!("{l}"));
                }
                Outcome::InterfaceChanged { diff } => {
                    eprintln!("interface changed:");
                    diff.iter().for_each(|l| eprintln!("{l}"));
                    anyhow::bail!("rerun with --regen to regenerate the crate sources");
                }
            }
        }
        Cmd::Inspect { xsa } => {
            let x = Xsa::open(&xsa)?;
            let d = &x.design;
            println!("{} ({} {}, Vivado {})", d.name, d.arch, d.device, d.vivado_version);
            println!("interface hash: {}", d.interface_hash());
            for c in d.pl_clocks.iter().filter(|c| c.enabled) {
                println!("FCLK{}: {} Hz", c.index, c.freq_hz);
            }
            for r in &d.memory_ranges {
                let v = d.instance(&r.instance).map_or("?", |i| i.vlnv.as_str());
                println!("{:#010x}-{:#010x}  {}  ({v})", r.base, r.high, r.instance);
            }
        }
    }
    Ok(())
}
