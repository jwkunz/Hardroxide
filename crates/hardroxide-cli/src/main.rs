mod host;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hardroxide_gen::{Options, Platform, generate};
use hardroxide_hwh::Xsa;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "hardroxide",
    version,
    about = "Generate Rust HAL crates from Vivado XSA files"
)]
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
        /// Kind of crate: linux (default), baremetal or rtos.
        #[arg(long, default_value = "linux")]
        platform: Platform,
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
    /// Run a bare-metal program on a board over JTAG (needs the Vitis tools).
    ///
    /// Resets the board, runs the XSA's own ps7_init, programs the PL, loads the ELF, starts
    /// it and shows the UART console. The board must be in JTAG boot mode.
    Run {
        /// The program (an ELF built for armv7a-none-eabihf).
        elf: PathBuf,
        /// The XSA of the design it runs on (supplies ps7_init and the bitstream).
        #[arg(long)]
        xsa: PathBuf,
        /// Console serial port.
        #[arg(long, default_value = "/dev/ttyUSB1")]
        tty: PathBuf,
        /// Seconds to show the console after the program starts.
        #[arg(long, default_value_t = 10)]
        watch: u64,
        /// Do not program the PL (the design has none, or it is already configured).
        #[arg(long)]
        no_pl: bool,
    },
    /// Build a BOOT.BIN that starts a bare-metal program on its own from SD or QSPI (needs the Vitis tools).
    ///
    /// Packs the vendor's first-stage loader (built from the XSA, so it runs the design's own PS
    /// setup), the bitstream and the program with bootgen.
    Package {
        /// The program (an ELF built for armv7a-none-eabihf).
        elf: PathBuf,
        /// The XSA of the design it runs on.
        #[arg(long)]
        xsa: PathBuf,
        /// Output file.
        #[arg(short, long, default_value = "BOOT.BIN")]
        output: PathBuf,
        /// Use this first-stage loader (an ELF) instead of building the vendor's.
        #[arg(long)]
        fsbl: Option<PathBuf>,
        /// Leave the bitstream out (the design has no PL, or the program loads it).
        #[arg(long)]
        no_pl: bool,
    },
    /// Check that the host can drive a board: Vitis tools, JTAG cable access, console port.
    Doctor {
        /// Console serial port.
        #[arg(long, default_value = "/dev/ttyUSB1")]
        tty: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Generate {
            xsa,
            out,
            name,
            hardroxide_path,
            platform,
        } => {
            let x = Xsa::open(&xsa).with_context(|| format!("reading {}", xsa.display()))?;
            let hardroxide_path = hardroxide_path
                .map(|p| p.canonicalize().with_context(|| format!("{}", p.display())))
                .transpose()?;
            let g = generate(
                &x,
                &Options {
                    platform,
                    crate_name: name.clone(),
                    hardroxide_path,
                },
            )?;
            let out =
                out.unwrap_or_else(|| PathBuf::from(name.unwrap_or_else(|| x.design.name.clone())));
            g.write_to(&out)?;
            println!("generated {} files in {}", g.files.len(), out.display());
        }
        Cmd::Update {
            xsa,
            crate_dir,
            regen,
        } => {
            use hardroxide_gen::update::{Outcome, update};
            let x = Xsa::open(&xsa).with_context(|| format!("reading {}", xsa.display()))?;
            match update(&crate_dir, &x, regen)? {
                Outcome::Unchanged => println!("up to date"),
                Outcome::BitstreamReplaced {
                    old_sha256,
                    new_sha256,
                } => {
                    println!(
                        "interface unchanged; bitstream replaced\n  {old_sha256}\n  {new_sha256}"
                    );
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
        Cmd::Run {
            elf,
            xsa,
            tty,
            watch,
            no_pl,
        } => host::run(&host::RunOptions {
            elf: &elf,
            xsa: &xsa,
            tty: &tty,
            watch_secs: watch,
            program_pl: !no_pl,
        })?,
        Cmd::Package {
            elf,
            xsa,
            output,
            fsbl,
            no_pl,
        } => host::package(&host::PackageOptions {
            elf: &elf,
            xsa: &xsa,
            out: &output,
            fsbl: fsbl.as_deref(),
            no_pl,
        })?,
        Cmd::Doctor { tty } => {
            let checks = host::doctor(&tty);
            let mut bad = 0;
            for c in &checks {
                println!(
                    "[{}] {}: {}",
                    if c.ok { " ok " } else { "FAIL" },
                    c.what,
                    c.detail
                );
                bad += usize::from(!c.ok);
            }
            if bad > 0 {
                anyhow::bail!("{bad} check(s) failed");
            }
        }
        Cmd::Inspect { xsa } => {
            let x = Xsa::open(&xsa)?;
            let d = &x.design;
            println!(
                "{} ({} {}, Vivado {})",
                d.name, d.arch, d.device, d.vivado_version
            );
            println!("interface hash: {}", d.interface_hash());
            for c in d.pl_clocks.iter().filter(|c| c.enabled) {
                println!("FCLK{}: {} Hz", c.index, c.freq_hz);
            }
            let sum = hardroxide_gen::summarize(d);
            println!("\nreachable from the PS (a field on the generated `Design`):");
            let (mut drv, mut hls, mut raw) = (0, 0, 0);
            for i in &sum.instances {
                let what = match &i.kind {
                    hardroxide_gen::Kind::Driver(t) => {
                        drv += 1;
                        t.clone()
                    }
                    hardroxide_gen::Kind::Hls(t) => {
                        hls += 1;
                        format!("{t} (HLS)")
                    }
                    hardroxide_gen::Kind::Raw => {
                        raw += 1;
                        "raw register window".into()
                    }
                };
                println!(
                    "{:#010x}  {:>6} KiB  {}  ->  {what}  ({})",
                    i.base,
                    i.size.div_ceil(1024),
                    i.field,
                    i.vlnv
                );
                for irq in &i.interrupts {
                    println!("                           irq {irq}");
                }
            }
            println!(
                "\n{} instances: {drv} with a curated driver, {hls} HLS wrapper(s), {raw} raw register window(s)",
                sum.instances.len()
            );
            if !sum.not_host_visible.is_empty() {
                println!(
                    "{} instance(s) are not reachable from the PS (private to another master such as a MicroBlaze) and get no field: {}",
                    sum.not_host_visible.len(),
                    sum.not_host_visible.join(", ")
                );
            }
        }
    }
    Ok(())
}
