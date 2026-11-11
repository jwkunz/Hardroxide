//! Bitstream loading (Linux `fpga_manager` sysfs) and PL clock control
//! (Zynq-7000 SLCR registers over an [`Mmio`](hardroxide_mmio::Mmio) window).
//!
//! Sysfs paths are configurable so the logic can be tested on a fake tree. The real-hardware behaviour of these interfaces on the stock PYNQ-Z2 image is UNVERIFIED; see
//! `docs/08-verification-gaps.md`.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum FpgaError {
    #[error("bitstream hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("fpga_manager did not reach `operating` state after load (state: `{0}`)")]
    NotOperating(String),
    #[error("PL clock FCLK{0} does not exist (valid: 0..=3)")]
    BadClockIndex(u8),
    #[error(
        "cannot reach {requested} Hz on FCLK{index}: best is {best} Hz from a {source_hz} Hz source"
    )]
    ClockUnreachable {
        index: u8,
        requested: u64,
        best: u64,
        source_hz: u64,
    },
    #[error("PLL feeding FCLK{0} is bypassed or has a zero feedback divider")]
    PllNotRunning(u8),
    #[error(transparent)]
    Mmio(#[from] hardroxide_mmio::MmioError),
}

fn io<T>(path: &Path, r: std::io::Result<T>) -> Result<T, FpgaError> {
    r.map_err(|source| FpgaError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Locations of the kernel interfaces used for programming the PL.
#[derive(Debug, Clone)]
pub struct FpgaPaths {
    /// e.g. `/sys/class/fpga_manager/fpga0`
    pub manager: PathBuf,
    /// Directory the kernel searches for firmware, normally `/lib/firmware`.
    pub firmware_dir: PathBuf,
}

impl Default for FpgaPaths {
    fn default() -> Self {
        Self {
            manager: "/sys/class/fpga_manager/fpga0".into(),
            firmware_dir: "/lib/firmware".into(),
        }
    }
}

/// Program the PL with a raw `.bin`, after verifying its SHA-256.
///
/// The image is written under `firmware_dir` and its name handed to
/// `fpga_manager`'s `firmware` attribute (full reconfiguration: `flags` = 0).
pub fn load_bitstream(
    paths: &FpgaPaths,
    bin: &[u8],
    expected_sha256: &str,
) -> Result<(), FpgaError> {
    let actual = sha256_hex(bin);
    if actual != expected_sha256 {
        return Err(FpgaError::HashMismatch {
            expected: expected_sha256.into(),
            actual,
        });
    }
    let name = format!("hardroxide-{}.bin", &actual[..16]);
    let fw = paths.firmware_dir.join(&name);
    io(&fw, std::fs::write(&fw, bin))?;
    let flags = paths.manager.join("flags");
    io(&flags, std::fs::write(&flags, "0"))?;
    let firmware = paths.manager.join("firmware");
    io(&firmware, std::fs::write(&firmware, &name))?;
    let state_p = paths.manager.join("state");
    let state = io(&state_p, std::fs::read_to_string(&state_p))?;
    let _ = std::fs::remove_file(&fw);
    if state.trim() != "operating" {
        return Err(FpgaError::NotOperating(state.trim().into()));
    }
    Ok(())
}

pub mod slcr;

/// Load-time configuration stored beside the bitstream (not part of the
/// interface hash): PS reference clock and FCLK targets.
///
/// Text format, one `key=value` per line: `ps_clk_hz=50000000`,
/// `fclk0=100000000`, ...
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadConfig {
    pub ps_clk_hz: u64,
    /// `(FCLK index, Hz)`, ascending by index.
    pub fclks: Vec<(u8, u64)>,
}

impl LoadConfig {
    pub fn to_text(&self) -> String {
        let mut s = format!("ps_clk_hz={}\n", self.ps_clk_hz);
        let mut f = self.fclks.clone();
        f.sort();
        for (i, hz) in f {
            s.push_str(&format!("fclk{i}={hz}\n"));
        }
        s
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut c = LoadConfig::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or(format!("line {}: expected key=value", n + 1))?;
            let v: u64 = v
                .trim()
                .parse()
                .map_err(|_| format!("line {}: bad number", n + 1))?;
            match k.trim() {
                "ps_clk_hz" => c.ps_clk_hz = v,
                k => match k.strip_prefix("fclk").and_then(|i| i.parse::<u8>().ok()) {
                    Some(i) if i < 4 => c.fclks.push((i, v)),
                    _ => return Err(format!("line {}: unknown key `{k}`", n + 1)),
                },
            }
        }
        c.fclks.sort();
        Ok(c)
    }

    /// Program every configured FCLK through the SLCR window.
    pub fn apply(&self, slcr: &impl hardroxide_mmio::Mmio) -> Result<(), FpgaError> {
        for &(i, hz) in &self.fclks {
            slcr::set_pl_clock(slcr, self.ps_clk_hz, i, hz)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_config_roundtrip() {
        let c = LoadConfig {
            ps_clk_hz: 50_000_000,
            fclks: vec![(1, 142_857_143), (0, 100_000_000)],
        };
        let t = c.to_text();
        assert_eq!(t, "ps_clk_hz=50000000\nfclk0=100000000\nfclk1=142857143\n");
        assert_eq!(LoadConfig::parse(&t).unwrap().to_text(), t);
        assert!(LoadConfig::parse("fclk9=1").is_err());
        assert!(LoadConfig::parse("nonsense").is_err());
    }

    fn fake() -> (PathBuf, FpgaPaths) {
        let root = std::env::temp_dir().join(format!(
            "hrx-fpga-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let p = FpgaPaths {
            manager: root.join("mgr"),
            firmware_dir: root.join("fw"),
        };
        std::fs::create_dir_all(&p.manager).unwrap();
        std::fs::create_dir_all(&p.firmware_dir).unwrap();
        std::fs::write(p.manager.join("state"), "operating\n").unwrap();
        (root, p)
    }

    #[test]
    fn loads_and_rejects_bad_hash() {
        let (root, p) = fake();
        let bin = b"\x01\x02\x03\x04";
        assert!(matches!(
            load_bitstream(&p, bin, "00"),
            Err(FpgaError::HashMismatch { .. })
        ));
        load_bitstream(&p, bin, &sha256_hex(bin)).unwrap();
        assert_eq!(
            std::fs::read_to_string(p.manager.join("flags")).unwrap(),
            "0"
        );
        assert!(
            std::fs::read_to_string(p.manager.join("firmware"))
                .unwrap()
                .starts_with("hardroxide-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
