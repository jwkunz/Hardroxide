//! The Linux side for Zynq UltraScale+: the port-width file of a generated crate, and loading a bitstream through the
//! same `fpga_manager` interface as the Zynq-7000, checking this family's PL addresses for drivers first.

use super::{Port, Width, set_port_width};
use crate::{FpgaError, FpgaPaths, load_bitstream_checking};
use hardroxide_mmio::MmioBackend;
use std::ops::RangeInclusive;

/// Where the processor sees the PL: the low-power master port, the two full-power ones, and their windows above 4 GiB.
pub const PL_ADDRESSES: [RangeInclusive<u64>; 3] = [
    0x8000_0000..=0xBFFF_FFFF,
    0x4_0000_0000..=0x5_FFFF_FFFF,
    0x10_0000_0000..=0x7F_FFFF_FFFF,
];

/// Program the PL with a raw `.bin` (the bitstream's words byte-swapped), as [`crate::load_bitstream`] does on the
/// Zynq-7000: verify its SHA-256, refuse while a kernel driver is bound to a PL address, hand it to `fpga_manager`.
pub fn load_bitstream(
    paths: &FpgaPaths,
    bin: &[u8],
    expected_sha256: &str,
) -> Result<(), FpgaError> {
    load_bitstream_checking(paths, bin, expected_sha256, &PL_ADDRESSES)
}

/// The widths a design gives the ports it uses, stored beside its bitstream (like the clocks, not part of the interface
/// hash).
///
/// Text format, one port per line: `m0=128` (a port the processor masters), `s2=64` (one the PL masters).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PortConfig {
    pub widths: Vec<(Port, Width)>,
}

impl PortConfig {
    pub fn to_text(&self) -> String {
        let mut widths = self.widths.clone();
        widths.sort_by_key(|&(port, _)| port);
        widths
            .into_iter()
            .map(|(port, width)| {
                let (kind, n) = match port {
                    Port::Master(n) => ('m', n),
                    Port::Slave(n) => ('s', n),
                };
                format!("{kind}{n}={}\n", width.bits())
            })
            .collect()
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config = PortConfig::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || format!("line {}: expected a port and its width, like m0=128", n + 1);
            let (port, bits) = line.split_once('=').ok_or_else(bad)?;
            let port = port.trim();
            let number = port.get(1..).and_then(|v| v.parse().ok()).ok_or_else(bad)?;
            let port = match port.get(..1) {
                Some("m") => Port::Master(number),
                Some("s") => Port::Slave(number),
                _ => return Err(bad()),
            };
            let width = bits
                .trim()
                .parse()
                .ok()
                .and_then(Width::from_bits)
                .ok_or_else(bad)?;
            if !port.exists() {
                return Err(format!("line {}: there is no such port", n + 1));
            }
            config.widths.push((port, width));
        }
        config.widths.sort_by_key(|&(port, _)| port);
        Ok(config)
    }

    /// Give every listed port its width.
    pub fn apply(&self, backend: &impl MmioBackend) -> Result<(), FpgaError> {
        for &(port, width) in &self.widths {
            set_port_width(backend, port, width)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256_hex;
    use hardroxide_mmio::MockBackend;

    #[test]
    fn the_port_file_round_trips_and_applies() {
        let text = "m0=128\nm2=32\ns2=128\n";
        let config = PortConfig::parse(text).unwrap();
        assert_eq!(config.to_text(), text);
        assert_eq!(config.widths[1], (Port::Master(2), Width::Bits32));
        let be = MockBackend::new();
        config.apply(&be).unwrap();
        assert_eq!(be.peek(0xFD61_5000), 2 << 8);
        for bad in ["m3=32", "s0=48", "x0=32", "m=32", "nonsense"] {
            assert!(PortConfig::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_driver_at_a_pl_address_of_this_family_blocks_loading() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("hrx-zynqmp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = FpgaPaths {
            manager: root.join("mgr"),
            firmware_dir: root.join("fw"),
            platform_devices: root.join("platform"),
            refuse_bound_pl_drivers: true,
        };
        std::fs::create_dir_all(&paths.manager).unwrap();
        std::fs::create_dir_all(&paths.firmware_dir).unwrap();
        std::fs::write(paths.manager.join("state"), "operating\n").unwrap();
        let bound = |name: &str, driver: &str| {
            let d = paths.platform_devices.join(name);
            std::fs::create_dir_all(&d).unwrap();
            symlink(
                format!("../../../bus/platform/drivers/{driver}"),
                d.join("driver"),
            )
            .unwrap();
        };
        let bin = b"\x01\x02\x03\x04";
        // The image's own interrupt device, and a peripheral of the processor, do not matter...
        bound("a0000000.fabric", "uio_pdrv_genirq");
        bound("ff0a0000.gpio", "zynqmp-gpio");
        load_bitstream(&paths, bin, &sha256_hex(bin)).unwrap();
        // ...a driver in the PL's windows does.
        for name in ["80010000.gpio", "400000000.dma", "1000000000.memory"] {
            bound(name, "some-driver");
            let e = load_bitstream(&paths, bin, &sha256_hex(bin)).unwrap_err();
            assert!(
                matches!(e, FpgaError::PlInUse(ref s) if s.contains(name)),
                "{e}"
            );
            std::fs::remove_dir_all(paths.platform_devices.join(name)).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
