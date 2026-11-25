//! Buses that record what is sent and answer from a script.

use core::convert::Infallible;
use embedded_hal::i2c::{self, I2c, Operation as I2cOp};
use embedded_hal::spi::{self, Operation as SpiOp, SpiDevice};
use std::collections::VecDeque;
use std::vec::Vec;

/// What went over the I2C bus.
#[derive(Debug, PartialEq, Eq)]
pub enum Bus {
    Write(u8, Vec<u8>),
    Read(u8, usize),
}

#[derive(Default)]
pub struct MockI2c {
    pub log: Vec<Bus>,
    pub answers: VecDeque<Vec<u8>>,
}

impl i2c::ErrorType for MockI2c {
    type Error = Infallible;
}

impl I2c for MockI2c {
    fn transaction(&mut self, address: u8, ops: &mut [I2cOp<'_>]) -> Result<(), Infallible> {
        for op in ops {
            match op {
                I2cOp::Write(bytes) => self.log.push(Bus::Write(address, bytes.to_vec())),
                I2cOp::Read(back) => {
                    self.log.push(Bus::Read(address, back.len()));
                    let answer = self.answers.pop_front().expect("an unexpected read");
                    back.copy_from_slice(&answer);
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct MockSpi {
    /// The bytes written in each transaction (with the chip selected throughout).
    pub sent: Vec<Vec<u8>>,
    pub answers: VecDeque<Vec<u8>>,
}

impl spi::ErrorType for MockSpi {
    type Error = Infallible;
}

impl SpiDevice for MockSpi {
    fn transaction(&mut self, ops: &mut [SpiOp<'_, u8>]) -> Result<(), Infallible> {
        let mut sent = Vec::new();
        for op in ops {
            match op {
                SpiOp::Write(bytes) => sent.extend_from_slice(bytes),
                SpiOp::Read(back) => {
                    let answer = self.answers.pop_front().expect("an unexpected read");
                    back.copy_from_slice(&answer);
                }
                SpiOp::DelayNs(_) => {}
                _ => unimplemented!("the drivers only write and read"),
            }
        }
        self.sent.push(sent);
        Ok(())
    }
}
