//! Running a bare-metal program on a board over JTAG, with the vendor's tools.
//!
//! `hardroxide run` resets the board, runs the design's own `ps7_init.tcl` (Zynq-7000) or
//! `psu_init.tcl` (Zynq UltraScale+) from its XSA, at run time, so no vendor tables are
//! redistributed, programs the PL, loads the ELF and starts it, then streams the UART
//! console. The XSA decides which of the two it is. The Vitis tools (`xsdb`, `hw_server`) do
//! the JTAG work; when they are missing the commands say so and stop.

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
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

/// The processor family of a design, told by which initialisation script its XSA carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Zynq7000,
    UltraScalePlus,
}

impl Family {
    pub fn init_script(self) -> &'static str {
        match self {
            Family::Zynq7000 => "ps7_init.tcl",
            Family::UltraScalePlus => "psu_init.tcl",
        }
    }
}

/// The files `run` needs from an XSA, extracted to `dir`: the PS initialisation script and
/// the `.bit`.
pub fn extract_xsa(xsa: &Path, dir: &Path) -> Result<Family> {
    std::fs::create_dir_all(dir)?;
    let mut z =
        zip::ZipArchive::new(std::fs::File::open(xsa).with_context(|| xsa.display().to_string())?)?;
    let (mut family, mut got_bit) = (None, false);
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        let name = f.name().to_string();
        let found = [Family::Zynq7000, Family::UltraScalePlus]
            .into_iter()
            .find(|f| name == f.init_script());
        let target = if let Some(found) = found {
            family = Some(found);
            found.init_script()
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
    let Some(family) = family else {
        bail!(
            "{} has no ps7_init.tcl or psu_init.tcl: the PS cannot be initialised over JTAG",
            xsa.display()
        );
    };
    if !got_bit {
        bail!(
            "{} has no bitstream (export with --include_bit)",
            xsa.display()
        );
    }
    Ok(family)
}

/// The `xsdb` script: reset, initialise the PS, program the PL, load and start the ELF.
pub fn run_script(family: Family, dir: &Path, elf: &Path, program_pl: bool) -> String {
    if family == Family::UltraScalePlus {
        return run_script_ultrascale(dir, elf, program_pl);
    }
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

/// The same for a Zynq UltraScale+, in the order the vendor's debug launch uses: the PL is
/// programmed before the PS is initialised, and core 0 is taken out of reset (at EL3) to
/// receive the program.
fn run_script_ultrascale(dir: &Path, elf: &Path, program_pl: bool) -> String {
    let pl = if program_pl {
        "targets -set -nocase -filter {name =~ \"PSU\"}\nfpga -file {DIR}/design.bit\n"
    } else {
        ""
    };
    format!(
        "connect -url tcp:localhost:{HW_SERVER_PORT}\n\
         targets -set -nocase -filter {{name =~ \"APU*\"}}\n\
         rst -system\nafter 3000\n\
         {pl}\
         targets -set -nocase -filter {{name =~ \"APU*\"}}\n\
         configparams force-mem-access 1\n\
         source {{DIR}}/psu_init.tcl\npsu_init\nafter 1000\npsu_post_config\n\
         psu_ps_pl_isolation_removal\nafter 1000\npsu_ps_pl_reset_config\n\
         targets -set -nocase -filter {{name =~ \"*Cortex-A53*#0\"}}\n\
         rst -processor\n\
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

/// When the console reader stops: not while the program is still being loaded (a large bitstream takes half a minute
/// over JTAG, longer than a short watch), then `watch` after it started, and no sooner than 8 s more than `watch` after
/// the reader did.
#[derive(Clone)]
struct ConsoleEnd {
    earliest: Instant,
    /// `None` while loading.
    end: Arc<Mutex<Option<Instant>>>,
}

impl ConsoleEnd {
    fn new(watch: Duration) -> Self {
        Self {
            earliest: Instant::now() + watch + Duration::from_secs(8),
            end: Arc::new(Mutex::new(None)),
        }
    }

    fn set(&self, end: Instant) {
        *self.end.lock().unwrap_or_else(|e| e.into_inner()) = Some(end);
    }

    fn program_started(&self, watch: Duration) {
        self.set(self.earliest.max(Instant::now() + watch));
    }

    /// Stop now: nothing was started.
    fn abandon(&self) {
        self.set(Instant::now());
    }

    fn reached(&self) -> bool {
        let end = *self.end.lock().unwrap_or_else(|e| e.into_inner());
        end.is_some_and(|end| Instant::now() >= end)
    }
}

/// Stream the console UART to stdout from `tty` (115200 8N1) until `end`.
fn spawn_console(tty: &Path, end: ConsoleEnd) -> Option<std::thread::JoinHandle<()>> {
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
        let mut buf = [0u8; 256];
        while !end.reached() {
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
    let family = extract_xsa(o.xsa, &tmp)?;
    ensure_hw_server(&v)?;
    let elf = o.elf.canonicalize()?;
    let script = tmp.join("run.tcl");
    std::fs::write(&script, run_script(family, &tmp, &elf, o.program_pl))?;
    // The console reader must exist before the program starts printing.
    let watch = Duration::from_secs(o.watch_secs);
    let end = ConsoleEnd::new(watch);
    let console = spawn_console(o.tty, end.clone());
    eprintln!("loading {} ...", o.elf.display());
    let st = v
        .shell(&format!("xsdb '{}'", script.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("running xsdb")?;
    if !st.status.success() {
        end.abandon();
        let _ = std::fs::remove_dir_all(&tmp);
        bail!(
            "xsdb failed (is the board in JTAG boot mode and powered? try `hardroxide doctor`):\n{}",
            String::from_utf8_lossy(&st.stderr)
        );
    }
    end.program_started(watch);
    eprintln!("running; console below ({} s) ...", o.watch_secs);
    if let Some(h) = console {
        let _ = h.join();
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// The `xsct` script that builds the first-stage boot loader (the vendor's Zynq FSBL, which
/// runs the XSA's own PS initialisation, loads the PL and hands over to the program).
pub fn fsbl_script(ws: &Path, xsa: &Path) -> String {
    format!(
        "setws {{{}}}\nplatform create -name hrx_plat -hw {{{}}} -proc ps7_cortexa9_0 -os standalone\nplatform generate\n",
        ws.display(),
        xsa.display()
    )
}

/// The Boot Image Format file bootgen reads: FSBL, then (optionally) the bitstream, then the program.
pub fn bif(fsbl: &Path, bit: Option<&Path>, elf: &Path) -> String {
    let mut s = format!("the_ROM_image:\n{{\n    [bootloader] {}\n", fsbl.display());
    if let Some(b) = bit {
        s.push_str(&format!("    {}\n", b.display()));
    }
    s.push_str(&format!("    {}\n}}\n", elf.display()));
    s
}

pub struct PackageOptions<'a> {
    pub elf: &'a Path,
    pub xsa: &'a Path,
    pub out: &'a Path,
    /// A first-stage loader to use instead of building the vendor's.
    pub fsbl: Option<&'a Path>,
    pub no_pl: bool,
}

/// Build a `BOOT.BIN` that boots `elf` on its own from SD or QSPI flash.
pub fn package(o: &PackageOptions) -> Result<()> {
    let v = require_vitis()?;
    if !o.elf.is_file() {
        bail!("{}: no such program", o.elf.display());
    }
    for tool in ["bootgen", "xsct"] {
        if !v.dir.join("bin").join(tool).exists() {
            bail!(
                "{tool} not found in {}/bin ({})",
                v.dir.display(),
                MISSING_VITIS
            );
        }
    }
    let tmp = std::env::temp_dir().join(format!("hrx-package-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    if extract_xsa(o.xsa, &tmp)? != Family::Zynq7000 {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("`package` builds boot images for the Zynq-7000 only so far; use `run` over JTAG");
    }
    let fsbl = match o.fsbl {
        Some(f) => f.canonicalize().with_context(|| f.display().to_string())?,
        None => {
            eprintln!("building the first-stage loader (about 20 s) ...");
            let ws = tmp.join("ws");
            let script = tmp.join("fsbl.tcl");
            std::fs::write(&script, fsbl_script(&ws, &o.xsa.canonicalize()?))?;
            let out = v
                .shell(&format!("xsct '{}'", script.display()))
                .output()
                .context("running xsct")?;
            let built = ws.join("hrx_plat/export/hrx_plat/sw/hrx_plat/boot/fsbl.elf");
            if !built.is_file() {
                bail!(
                    "xsct did not produce an FSBL:\n{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            built
        }
    };
    let elf = o.elf.canonicalize()?;
    let bit = tmp.join("design.bit");
    let bif_path = tmp.join("boot.bif");
    std::fs::write(
        &bif_path,
        bif(&fsbl, (!o.no_pl).then_some(bit.as_path()), &elf),
    )?;
    let out = o.out.to_path_buf();
    let st = v
        .shell(&format!(
            "bootgen -arch zynq -image '{}' -o '{}' -w on",
            bif_path.display(),
            out.display()
        ))
        .output()
        .context("running bootgen")?;
    let _ = std::fs::remove_dir_all(&tmp);
    if !st.status.success() {
        bail!(
            "bootgen failed:\n{}\n{}",
            String::from_utf8_lossy(&st.stdout),
            String::from_utf8_lossy(&st.stderr)
        );
    }
    eprintln!(
        "wrote {} ({} bytes). Copy it to the root of a FAT-formatted SD card, set the board's boot jumper to SD, and power-cycle.",
        out.display(),
        std::fs::metadata(&out)?.len()
    );
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
    fn the_console_outlasts_a_slow_load() {
        // Nothing ends it while loading, however short the watch.
        let end = ConsoleEnd::new(Duration::ZERO);
        assert!(!end.reached());
        // Once the program runs it gets its watch time.
        end.program_started(Duration::from_secs(60));
        assert!(!end.reached());
        // A quick load never shortens the time to less than before (watch and 8 s from the start).
        let end = ConsoleEnd::new(Duration::ZERO);
        end.program_started(Duration::ZERO);
        assert!(!end.reached());
        end.abandon();
        assert!(end.reached());
    }

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
        let zynq = |pl| run_script(Family::Zynq7000, Path::new("/t"), Path::new("/e.elf"), pl);
        let s = zynq(true);
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
        assert!(!zynq(false).contains("fpga"));
    }

    #[test]
    fn the_ultrascale_script_programs_the_pl_first_and_loads_into_a_fresh_core() {
        let script = |pl| {
            run_script(
                Family::UltraScalePlus,
                Path::new("/t"),
                Path::new("/e.elf"),
                pl,
            )
        };
        let s = script(true);
        let order = [
            "rst -system",
            "fpga -file /t/design.bit",
            "source /t/psu_init.tcl",
            "\npsu_init\n",
            "psu_post_config",
            "psu_ps_pl_isolation_removal",
            "psu_ps_pl_reset_config",
            "Cortex-A53*#0",
            "rst -processor",
            "dow /e.elf",
            "con",
        ];
        let mut at = 0;
        for k in order {
            let i = s[at..]
                .find(k)
                .unwrap_or_else(|| panic!("missing or out of order: {k}\n{s}"));
            at += i + k.len() - usize::from(k.ends_with('\n'));
        }
        assert!(!s.contains("ps7"));
        assert!(!script(false).contains("fpga"));
    }

    #[test]
    fn extracting_an_xsa_requires_ps7_init_and_a_bitstream() {
        let d = std::env::temp_dir().join(format!("hrx-xsa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let xsa = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pynq_z2_gpio.xsa");
        assert_eq!(extract_xsa(&xsa, &d).unwrap(), Family::Zynq7000);
        assert!(d.join("ps7_init.tcl").is_file() && d.join("design.bit").is_file());
        let rfsoc =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rfsoc4x2/rfsoc4x2_gpio.xsa");
        let u = d.join("u");
        assert_eq!(extract_xsa(&rfsoc, &u).unwrap(), Family::UltraScalePlus);
        assert!(u.join("psu_init.tcl").is_file() && u.join("design.bit").is_file());
        // The base overlay XSA was repackaged from .bit/.hwh only: no ps7_init.
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/real/pynq_base_overlay.xsa");
        assert!(extract_xsa(&base, &d.join("b")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
