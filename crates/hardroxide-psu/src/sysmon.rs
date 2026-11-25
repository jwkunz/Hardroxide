//! The system monitors' temperature sensors: one in the processing system, one in the programmable logic.
//!
//! The monitors convert continuously from reset (their default sequence); this only reads the results. The conversion is
//! the vendor driver's for the on-chip reference.

use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

// Offsets from the monitor block (`map::SYSMON_BASE`).
const PS_TEMPERATURE: usize = 0x800;
const PL_TEMPERATURE: usize = 0xC00;

/// Which die sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensor {
    /// Beside the processors.
    ProcessingSystem,
    /// In the programmable logic.
    ProgrammableLogic,
}

/// A temperature in thousandths of a degree Celsius from a 16-bit reading: `reading / 65536 * 509.314 - 280.231`.
pub fn millicelsius(reading: u16) -> i32 {
    (i64::from(reading) * 509_314 / 65_536 - 280_231) as i32
}

/// The temperature at `sensor`, in thousandths of a degree Celsius; `None` if the monitor has no result (reads zero).
/// `regs` maps the monitor block.
pub fn temperature(regs: &impl Mmio, sensor: Sensor) -> Result<Option<i32>> {
    let offset = match sensor {
        Sensor::ProcessingSystem => PS_TEMPERATURE,
        Sensor::ProgrammableLogic => PL_TEMPERATURE,
    };
    let reading = regs.read32(offset)? as u16;
    Ok((reading != 0).then(|| millicelsius(reading)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    #[test]
    fn readings_convert_as_the_vendor_driver_does() {
        // The driver's formula in floating point, to a hundredth of a degree.
        for reading in [0x8000u16, 0x9C40, 0xB000, 0xFFFF] {
            let vendor = f64::from(reading) / 65536.0 / 0.001_963_425_31 - 280.2309;
            let ours = f64::from(millicelsius(reading)) / 1000.0;
            assert!(
                (vendor - ours).abs() < 0.01,
                "{reading:#x}: {vendor} {ours}"
            );
        }
        // 0x9F00 is a die at about 36 degrees.
        assert!((35_000..37_500).contains(&millicelsius(0x9F00)));
    }

    #[test]
    fn each_sensor_has_its_register_and_zero_means_no_result() {
        let be = MockBackend::new();
        let r = be.map(0xFFA5_0000, 0x1000).unwrap();
        be.poke(0xFFA5_0800, 0x9F00);
        assert!(temperature(&r, Sensor::ProcessingSystem).unwrap().is_some());
        assert_eq!(temperature(&r, Sensor::ProgrammableLogic).unwrap(), None);
    }
}
