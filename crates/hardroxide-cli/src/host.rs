//! Running a bare-metal program on a board over JTAG, with the vendor's tools.
//!
//! `hardroxide run` resets the board, runs the design's own `ps7_init.tcl` (from its XSA,
//! at run time, so no vendor tables are redistributed), programs the PL, loads the ELF
//! and starts it, then streams the UART console. The Vitis tools (`xsdb`, `hw_server`) do the
//! JTAG work; when they are missing the commands say so and stop.

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const HW_SERVER_PORT: u16 = 3121;

/// Directories that may contain a Vitis installation, most specific first.
fn vitis_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in ["HRX_VITIS", "XILINX_VITIS"] {
        if let Some(p) = std::env::var_os(var) {
            v.push(PathBuf::from(p));
        }
    }
    if let Some(p) = std::env::var_os("XILINX_ROOT") {
        v.push(PathBuf::from(p).join("Vitis"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            // .../Vitis/bin/xsdb -> .../Vitis
            if d.join("xsdb").exists()
                && let Some(p) = d.parent()
            {
                v.push(p.to_path_buf());
            }
        }
    }
    let mut roots = vec![PathBuf::from("/opt/Xilinx"), PathBuf::from("/tools/Xilinx")];
    if let Some(h) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(h).join("AMD"));
        roots.push(PathBuf::from(std::env::var_os("HOME").unwrap()).join("Xilinx"));
    }
    for r in roots {
        // <root>/Vitis/<version> (Xilinx layout) and <root>/<version>/Vitis (AMD layout)
        if let Ok(rd) = std::fs::read_dir(r.join("Vitis")) {
            v.extend(rd.flatten().map(|e| e.path()));
        }
        if let Ok(rd) = std::fs::read_dir(&r) {
            v.extend(rd.flatten().map(|e| e.path().join("Vitis")));
        }
    }
    v
}

/// A Vitis installation: the directory holding `settings64.sh` and `bin/xsdb`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vitis {
    pub dir: PathBuf,
}

impl Vitis {
    /// Whether `dir` looks like a Vitis installation.
    pub fn at(dir: &Path) -> Option<Self> {
        (dir.join("settings64.sh").is_file() && dir.join("bin/xsdb").exists()).then(|| Vitis {
            dir: dir.to_path_buf(),
        })
    }

    pub fn find() -> Option<Self> {
        vitis_candidates().iter().find_map(|d| Vitis::at(d))
    }

    /// A command running `script` (shell text) with the Vitis environment loaded.
    fn shell(&self, script: &str) -> Command {
        let mut c = Command::new("bash");
        c.arg("-c").arg(format!(
            "source '{}/settings64.sh' >/dev/null 2>&1; {script}",
            self.dir.display()
        ));
        c
    }
}

const MISSING_VITIS: &str = "the Vitis tools (xsdb, hw_server) were not found.\nInstall AMD Vitis 2025.2 (or set HRX_VITIS to the directory containing settings64.sh) and retry;\nthese commands drive the board through them and stop here without them";

pub fn require_vitis() -> Result<Vitis> {
    Vitis::find().context(MISSING_VITIS)
}

/// The files `run` needs from an XSA, extracted to `dir`: `ps7_init.tcl` and the `.bit`.
pub fn extract_xsa(xsa: &Path, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut z =
        zip::ZipArchive::new(std::fs::File::open(xsa).with_context(|| xsa.display().to_string())?)?;
    let (mut got_init, mut got_bit) = (false, false);
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        let name = f.name().to_string();
        let target = if name == "ps7_init.tcl" {
            got_init = true;
            "ps7_init.tcl"
        } else if name.ends_with(".bit") {
            got_bit = true;
            "design.bit"
        } else {
            continue;
        };
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        std::fs::write(dir.join(target), data)?;
    }
    if !got_init {
        bail!(
            "{} has no ps7_init.tcl: the PS cannot be initialised over JTAG",
            xsa.display()
        );
    }
    if !got_bit {
        bail!(
            "{} has no bitstream (export with --include_bit)",
            xsa.display()
        );
    }
    Ok(())
}

/// The `xsdb` script: reset, initialise the PS, program the PL, load and start the ELF.
pub fn run_script(dir: &Path, elf: &Path, program_pl: bool) -> String {
    let pl = if program_pl {
        "targets -set -nocase -filter {name =~ \"xc7z*\"}\nfpga -file {DIR}/design.bit\ntargets -set -nocase -filter {name =~ \"*Cortex-A9*#0\"}\n"
    } else {
        ""
    };
    format!(
        "connect -url tcp:localhost:{HW_SERVER_PORT}\n\
         targets -set -nocase -filter {{name =~ \"*Cortex-A9*#0\"}}\n\
         rst -system\nafter 2000\n\
         targets -set -nocase -filter {{name =~ \"*Cortex-A9*#0\"}}\n\
         catch {{stop}}\n\
         source {{DIR}}/ps7_init.tcl\nps7_init\n{pl}ps7_post_config\n\
         dow {{ELF}}\ncon\n"
    )
    .replace("{DIR}", &dir.display().to_string())
    .replace("{ELF}", &elf.display().to_string())
}

fn hw_server_up() -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], HW_SERVER_PORT).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

fn ensure_hw_server(v: &Vitis) -> Result<()> {
    if hw_server_up() {
        return Ok(());
    }
    v.shell(&format!("exec hw_server -p {HW_SERVER_PORT}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("starting hw_server")?;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10) {
        if hw_server_up() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    bail!("hw_server did not start (run `hardroxide doctor`)")
}

/// Stream the console UART to stdout from `tty` (115200 8N1) until `stop` elapses.
fn spawn_console(tty: &Path, stop: Duration) -> Option<std::thread::JoinHandle<()>> {
    if !tty.exists() {
        eprintln!(
            "note: {} not found; the program's console output will not be shown",
            tty.display()
        );
        return None;
    }
    let _ = Command::new("stty")
        .arg("-F")
        .arg(tty)
        .args(["115200", "raw", "-echo", "-hupcl", "min", "0", "time", "2"])
        .status();
    let tty = tty.to_path_buf();
    Some(std::thread::spawn(move || {
        use std::io::Write;
        let Ok(mut f) = std::fs::OpenOptions::new().read(true).open(&tty) else {
            return;
        };
        let t0 = Instant::now();
        let mut buf = [0u8; 256];
        while t0.elapsed() < stop {
            // Non-blocking-ish: a raw tty with VMIN=0 would be better; poll briefly instead.
            match f.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let mut o = std::io::stdout().lock();
                    let _ = o.write_all(&buf[..n]);
                    let _ = o.flush();
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }))
}

pub struct RunOptions<'a> {
    pub elf: &'a Path,
    pub xsa: &'a Path,
    pub tty: &'a Path,
    /// Seconds to keep showing the console after the program starts.
    pub watch_secs: u64,
    /// Program the PL from the XSA's bitstream.
    pub program_pl: bool,
}

pub fn run(o: &RunOptions) -> Result<()> {
    let v = require_vitis()?;
    if !o.elf.is_file() {
        bail!("{}: no such program", o.elf.display());
    }
    let tmp = std::env::temp_dir().join(format!("hrx-run-{}", std::process::id()));
    extract_xsa(o.xsa, &tmp)?;
    ensure_hw_server(&v)?;
    let elf = o.elf.canonicalize()?;
    let script = tmp.join("run.tcl");
    std::fs::write(&script, run_script(&tmp, &elf, o.program_pl))?;
    // The console reader must exist before the program starts printing.
    let console = spawn_console(o.tty, Duration::from_secs(o.watch_secs + 8));
    eprintln!("loading {} ...", o.elf.display());
    let st = v
        .shell(&format!("xsdb '{}'", script.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("running xsdb")?;
    if !st.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!(
            "xsdb failed (is the board in JTAG boot mode and powered? try `hardroxide doctor`):\n{}",
            String::from_utf8_lossy(&st.stderr)
        );
    }
    eprintln!("running; console below ({} s) ...", o.watch_secs);
    if let Some(h) = console {
        let _ = h.join();
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// One line of the `doctor` report.
pub struct Check {
    pub what: String,
    pub ok: bool,
    pub detail: String,
}

fn find_cable() -> Option<(String, bool)> {
    for e in std::fs::read_dir("/sys/bus/usb/devices").ok()?.flatten() {
        let p = e.path();
        let rd = |f: &str| {
            std::fs::read_to_string(p.join(f))
                .ok()
                .map(|s| s.trim().to_string())
        };
        if rd("idVendor").as_deref() == Some("0403") && rd("idProduct").as_deref() == Some("6010") {
            let (b, d) = (rd("busnum")?, rd("devnum")?);
            let node = format!(
                "/dev/bus/usb/{:03}/{:03}",
                b.parse::<u32>().ok()?,
                d.parse::<u32>().ok()?
            );
            let writable = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&node)
                .is_ok();
            return Some((node, writable));
        }
    }
    None
}

pub fn doctor(tty: &Path) -> Vec<Check> {
    let mut v = Vec::new();
    match Vitis::find() {
        Some(t) => v.push(Check {
            what: "Vitis tools".into(),
            ok: true,
            detail: format!("found at {}", t.dir.display()),
        }),
        None => v.push(Check {
            what: "Vitis tools".into(),
            ok: false,
            detail: MISSING_VITIS.replace('\n', " "),
        }),
    }
    match find_cable() {
        None => v.push(Check {
            what: "JTAG cable (FT2232, 0403:6010)".into(),
            ok: false,
            detail: "not found: connect the board's USB-JTAG/UART port and power the board".into(),
        }),
        Some((node, true)) => v.push(Check {
            what: "JTAG cable (FT2232, 0403:6010)".into(),
            ok: true,
            detail: format!("{node} is accessible"),
        }),
        Some((node, false)) => v.push(Check {
            what: "JTAG cable (FT2232, 0403:6010)".into(),
            ok: false,
            detail: format!("{node} is not writable by you. Install Vitis's udev rule once, as root:\n    sudo cp <Vitis>/../data/xicom/cable_drivers/lin64/install_script/install_drivers/52-xilinx-ftdi-usb.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger\nthen unplug and replug the board's USB cable"),
        }),
    }
    v.push(Check {
        what: format!("console port {}", tty.display()),
        ok: tty.exists(),
        detail: if tty.exists() {
            "present".into()
        } else {
            "missing (the UART interface of the cable; the program's output will not be shown)"
                .into()
        },
    });
    v.push(Check {
        what: "hw_server".into(),
        ok: true,
        detail: if hw_server_up() {
            format!("already running on port {HW_SERVER_PORT}")
        } else {
            "not running (started on demand by `hardroxide run`)".into()
        },
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vitis_directory_needs_settings_and_xsdb() {
        let d = std::env::temp_dir().join(format!("hrx-vitis-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("bin")).unwrap();
        assert_eq!(Vitis::at(&d), None);
        std::fs::write(d.join("settings64.sh"), "").unwrap();
        assert_eq!(Vitis::at(&d), None);
        std::fs::write(d.join("bin/xsdb"), "").unwrap();
        assert_eq!(Vitis::at(&d), Some(Vitis { dir: d.clone() }));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_script_resets_initialises_programs_loads_and_starts_in_order() {
        let s = run_script(Path::new("/t"), Path::new("/e.elf"), true);
        let order = [
            "rst -system",
            "source /t/ps7_init.tcl",
            "ps7_init",
            "fpga -file /t/design.bit",
            "ps7_post_config",
            "dow /e.elf",
            "con",
        ];
        let mut at = 0;
        for k in order {
            let i = s[at..]
                .find(k)
                .unwrap_or_else(|| panic!("missing or out of order: {k}\n{s}"));
            at += i + k.len();
        }
        assert!(!run_script(Path::new("/t"), Path::new("/e.elf"), false).contains("fpga"));
    }

    #[test]
    fn extracting_an_xsa_requires_ps7_init_and_a_bitstream() {
        let d = std::env::temp_dir().join(format!("hrx-xsa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let xsa = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pynq_z2_gpio.xsa");
        extract_xsa(&xsa, &d).unwrap();
        assert!(d.join("ps7_init.tcl").is_file() && d.join("design.bit").is_file());
        // The base overlay XSA was repackaged from .bit/.hwh only: no ps7_init.
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/real/pynq_base_overlay.xsa");
        assert!(extract_xsa(&base, &d.join("b")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
