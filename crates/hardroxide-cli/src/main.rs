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
