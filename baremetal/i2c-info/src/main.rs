//! Look at the RFSoC 4x2's I2C buses **without changing anything**: which addresses answer, and what the board's INA220
//! power monitors measure.
//!
//! The buses also carry devices that a stray write could upset (supplies, EEPROMs), so the program is restricted to two
//! kinds of transfer: reading a byte from an address, and, for the monitors the board's device tree names, writing the one
//! byte that selects which of a monitor's registers the following read returns. It never sends data after that byte.

#![no_std]
#![no_main]

use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};
use platform::ps::{self, cpu, println};
use ps::i2c::{I2c, I2cError};

#[allow(dead_code)]
#[path = "../../common/platform.rs"]
mod platform;

ps::entry!(main);

/// The INA220s on bus 0 and their shunt resistors in milliohms (the board's device tree).
const MONITORS: [(u8, u32); 9] = [
    (0x40, 1),
    (0x41, 1),
    (0x42, 1),
    (0x43, 10),
    (0x44, 10),
    (0x45, 10),
    (0x46, 10),
    (0x47, 10),
    (0x48, 10),
];
const CONFIGURATION: u8 = 0;
const SHUNT_VOLTAGE: u8 = 1;
const BUS_VOLTAGE: u8 = 2;

fn bus(n: usize) -> (I2c<PhysRegion>, u32) {
    // SAFETY: the controller's registers are used by nothing else; the clock registers are only read.
    let be = unsafe { PhysMem::new() };
    let base = [ps::map::I2C0_BASE, ps::map::I2C1_BASE][n];
    let input = if cfg!(feature = "qemu") {
        100_000_000
    } else {
        let crl = be
            .map(ps::map::CRL_APB_BASE as u64, ps::map::CRL_APB_SIZE)
            .unwrap();
        let crf = be
            .map(ps::map::CRF_APB_BASE as u64, ps::map::CRF_APB_SIZE)
            .unwrap();
        ps::clocks::i2c_ref_hz(&crl, &crf, 33_333_333, n).unwrap() as u32
    };
    let i2c = I2c::new(be.map(base as u64, ps::map::I2C_SIZE).unwrap());
    let hz = i2c.set_clock(input, 100_000).unwrap();
    (i2c, hz)
}

/// One register of a monitor: select it, then read its two bytes.
fn register(i2c: &I2c<PhysRegion>, address: u8, register: u8) -> Result<u16, I2cError> {
    let mut value = [0u8; 2];
    i2c.write_read(address, &[register], &mut value)?;
    Ok(u16::from_be_bytes(value))
}

fn main() -> ! {
    platform::init("i2c-info (read only)");
    for n in 0..2 {
        let (i2c, hz) = bus(n);
        ps::print!("bus {n} at {hz} Hz, answering addresses:");
        let (mut found, mut failed) = (0, 0);
        for address in 0x08..=0x77u8 {
            let mut byte = [0u8; 1];
            match i2c.read(address, &mut byte) {
                Ok(()) => {
                    ps::print!(" {address:#04x}");
                    found += 1;
                }
                Err(I2cError::Nack(_)) => {}
                Err(e) => {
                    if failed == 0 {
                        ps::print!(" [at {address:#04x}: {e}]");
                    }
                    failed += 1;
                }
            }
        }
        println!("  ({found} found, {failed} other errors)");
    }

    let (i2c, _) = bus(0);
    let mut read = 0;
    println!("power monitors on bus 0:");
    for (address, milliohms) in MONITORS {
        let result = register(&i2c, address, CONFIGURATION).and_then(|configuration| {
            let shunt = register(&i2c, address, SHUNT_VOLTAGE)? as i16;
            let bus_voltage = register(&i2c, address, BUS_VOLTAGE)?;
            Ok((configuration, shunt, bus_voltage))
        });
        match result {
            Ok((configuration, shunt, bus_voltage)) => {
                // The shunt voltage counts 10 uV, the bus voltage (bits 15:3) 4 mV.
                let millivolts = u32::from(bus_voltage >> 3) * 4;
                let milliamps = i32::from(shunt) * 10 / milliohms as i32;
                println!(
                    "  {address:#04x}: {millivolts:>5} mV {milliamps:>6} mA {:>6} mW  (configuration {configuration:#06x}, shunt {milliohms} mohm)",
                    i64::from(millivolts) * i64::from(milliamps) / 1000
                );
                read += 1;
            }
            Err(e) => println!("  {address:#04x}: {e}"),
        }
    }
    println!("{read} of {} monitors read", MONITORS.len());
    println!("I2C INFO DONE");
    loop {
        cpu::wfe();
    }
}
