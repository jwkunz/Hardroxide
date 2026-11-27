//! The two small blocks of the radio in the RFSoC base overlays: `xilinx.com:ip:amplitude_controller`,
//! which feeds a transmit converter, and `xilinx.com:ip:packet_generator`, which cuts a packet out of a
//! receiver's streams for two DMAs. Both are RFSoC-PYNQ's; their registers are as its drivers use them.

use crate::dma::{AxiDma, Direction, DmaIpError, Transfer};
use core::time::Duration;
use hardroxide_dma::DmaBuffer;
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{Mmio, MmioError};

const ENABLE: usize = 0x00;
const VALUE: usize = 0x04;
const LENGTH: usize = 0x00;
const SEND: usize = 0x04;
/// The DMA's stream-to-memory length register: after a transfer, the bytes that arrived.
const RECEIVED: usize = 0x58;
/// How long a record's DMAs must stay waiting, before the packet is asked for, to show that nothing old is on its way.
const SETTLE: Duration = Duration::from_micros(50);
/// How many times a DMA may find something old before giving up.
const STALE_LIMIT: usize = 16;

/// Samples (of 16 bits) in one word of a receiver's streams.
pub const SAMPLES_PER_WORD: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum RadioError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error(transparent)]
    Dma(#[from] DmaIpError),
    #[error("amplitude {0} is above full scale ({max})", max = AmplitudeController::<()>::FULL_SCALE)]
    Amplitude(u16),
    #[error("a packet is 2 to 4096 words of 8 samples, not {0} words")]
    PacketLength(usize),
    #[error("a record is 16 to 32768 samples in steps of 8, not {0}")]
    RecordLength(usize),
    #[error("a DMA received {got} bytes of a record of {wanted}: the packet did not arrive whole")]
    Short { got: usize, wanted: usize },
    #[error("data keeps arriving at a DMA although no packet was asked for")]
    Unrequested,
}

/// Puts one value, on both I and Q, into a transmit converter's stream for as long as it is on.
/// The converter's mixer makes the tone: its frequency is the mixer's.
pub struct AmplitudeController<R> {
    regs: R,
}

impl<R> AmplitudeController<R> {
    /// The largest amplitude.
    pub const FULL_SCALE: u16 = 0x7FFF;
}

impl<R: Mmio> AmplitudeController<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// The value sent while on, 0 to [`FULL_SCALE`](Self::FULL_SCALE).
    pub fn set_amplitude(&mut self, amplitude: u16) -> Result<(), RadioError> {
        if amplitude > Self::FULL_SCALE {
            return Err(RadioError::Amplitude(amplitude));
        }
        let both = u32::from(amplitude) << 16 | u32::from(amplitude);
        Ok(self.regs.write32(VALUE, both)?)
    }

    pub fn amplitude(&self) -> Result<u16, MmioError> {
        Ok(self.regs.read32(VALUE)? as u16)
    }

    pub fn enable(&mut self) -> Result<(), MmioError> {
        self.regs.write32(ENABLE, 1)
    }

    /// Send zeroes instead.
    pub fn disable(&mut self) -> Result<(), MmioError> {
        self.regs.write32(ENABLE, 0)
    }

    pub fn is_enabled(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(ENABLE)? & 1 != 0)
    }
}

/// Passes a set number of words of a receiver's I and Q streams to their DMAs, on request.
pub struct PacketGenerator<R> {
    regs: R,
}

impl<R: Mmio> PacketGenerator<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// The length of the next packet, in words of [`SAMPLES_PER_WORD`] samples.
    pub fn set_length(&mut self, words: usize) -> Result<(), RadioError> {
        if !(2..=4096).contains(&words) {
            return Err(RadioError::PacketLength(words));
        }
        Ok(self.regs.write32(LENGTH, words as u32)?)
    }

    pub fn length(&self) -> Result<usize, MmioError> {
        Ok(self.regs.read32(LENGTH)? as usize)
    }

    /// Send one packet. The DMAs that take it must be waiting already.
    pub fn send(&mut self) -> Result<(), MmioError> {
        // The block acts on the bit's rising edge.
        self.regs.write32(SEND, 0)?;
        self.regs.write32(SEND, 1)
    }

    /// Lower the request again.
    pub fn stop(&mut self) -> Result<(), MmioError> {
        self.regs.write32(SEND, 0)
    }
}

/// Record `samples` samples of one receiver: I into the first buffer, Q into the second, as
/// signed 16-bit little-endian numbers, each pair taken at the same instant. The buffers need
/// room for `2 * samples` bytes.
///
/// Both DMAs are started before the packet is asked for, as they must be. Anything an earlier
/// record that failed left on its way arrives first and is discarded. A record that does not
/// arrive whole is an error: a receiver whose path to memory cannot keep up loses the end of a
/// long one. On an error the DMAs are stopped and the buffers are not given back, as with
/// [`Transfer::wait`](crate::dma::Transfer::wait).
pub fn record<R: Mmio, B: DmaBuffer>(
    generator: &mut PacketGenerator<R>,
    i_dma: &AxiDma<R>,
    q_dma: &AxiDma<R>,
    buffers: (B, B),
    samples: usize,
    timeout: Duration,
) -> Result<(B, B), RadioError> {
    record_after(generator, i_dma, q_dma, buffers, samples, timeout, SETTLE)
}

/// [`record`] with the quiet time to wait for before the request.
fn record_after<R: Mmio, B: DmaBuffer>(
    generator: &mut PacketGenerator<R>,
    i_dma: &AxiDma<R>,
    q_dma: &AxiDma<R>,
    buffers: (B, B),
    samples: usize,
    timeout: Duration,
    settle: Duration,
) -> Result<(B, B), RadioError> {
    if !samples.is_multiple_of(SAMPLES_PER_WORD) || !(16..=32768).contains(&samples) {
        return Err(RadioError::RecordLength(samples));
    }
    let bytes = 2 * samples;
    generator.stop()?;
    generator.set_length(samples / SAMPLES_PER_WORD)?;
    let mut i = arm(i_dma, buffers.0, bytes)?;
    let mut q = arm(q_dma, buffers.1, bytes)?;
    let mut quiet_since = Instant::now();
    let mut stale = 0;
    while quiet_since.elapsed() < settle {
        if i.poll()? {
            let buffer = i.wait(Duration::ZERO)?;
            i = arm(i_dma, buffer, bytes)?;
        } else if q.poll()? {
            let buffer = q.wait(Duration::ZERO)?;
            q = arm(q_dma, buffer, bytes)?;
        } else {
            core::hint::spin_loop();
            continue;
        }
        stale += 1;
        if stale > STALE_LIMIT {
            return Err(RadioError::Unrequested);
        }
        quiet_since = Instant::now();
    }
    generator.send()?;
    let i = i.wait(timeout);
    let q = q.wait(timeout);
    generator.stop()?;
    let buffers = (i?, q?);
    for dma in [i_dma, q_dma] {
        let got = dma.regs().read32(RECEIVED)? as usize;
        if got != bytes {
            return Err(RadioError::Short { got, wanted: bytes });
        }
    }
    Ok(buffers)
}

fn arm<R: Mmio, B: DmaBuffer>(
    dma: &AxiDma<R>,
    buffer: B,
    bytes: usize,
) -> Result<Transfer<'_, R, B>, RadioError> {
    Ok(dma
        .start(Direction::StreamToMem, buffer, bytes)
        .map_err(|e| e.error)?)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_dma::MockBuffer;
    use hardroxide_mmio::{MmioBackend, MockAccess, MockBackend};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    const GENERATOR: u64 = 0xB002_0000;
    const I_DMA: u64 = 0xB001_0000;
    const Q_DMA: u64 = 0xB000_0000;

    #[test]
    fn the_amplitude_goes_on_both_halves() {
        let be = MockBackend::new();
        let mut a = AmplitudeController::new(be.map(0xB010_0000, 0x1000).unwrap());
        a.set_amplitude(0x1234).unwrap();
        assert_eq!(be.peek(0xB010_0004), 0x1234_1234);
        assert_eq!(a.amplitude().unwrap(), 0x1234);
        assert!(matches!(
            a.set_amplitude(0x8000),
            Err(RadioError::Amplitude(0x8000))
        ));
        assert_eq!(be.peek(0xB010_0004), 0x1234_1234);
        a.enable().unwrap();
        assert!(a.is_enabled().unwrap());
        a.disable().unwrap();
        assert!(!a.is_enabled().unwrap());
    }

    #[test]
    fn a_record_arms_both_dmas_before_asking_for_the_packet() {
        let be = MockBackend::new();
        for dma in [I_DMA, Q_DMA] {
            // Halted until started; the status flags clear by writing them.
            be.poke(dma + 0x34, 1);
            be.set_write1_clear(dma + 0x34);
        }
        let map = |at| be.map(at, 0x1000).unwrap();
        let mut generator = PacketGenerator::new(map(GENERATOR));
        let i_dma = AxiDma::with_length_width(map(I_DMA), 26);
        let q_dma = AxiDma::with_length_width(map(Q_DMA), 26);
        // Completes both transfers once the packet has been asked for.
        let hardware = {
            let be = be.clone();
            std::thread::spawn(move || {
                while be.peek(GENERATOR + 4) != 1 {
                    std::thread::yield_now();
                }
                for dma in [I_DMA, Q_DMA] {
                    be.poke(dma + 0x34, (1 << 12) | 2);
                }
            })
        };
        let buffers = (
            MockBuffer::new(65536, 0x1000_0000),
            MockBuffer::new(65536, 0x1100_0000),
        );
        let (i, q) = record(
            &mut generator,
            &i_dma,
            &q_dma,
            buffers,
            32768,
            Duration::from_secs(5),
        )
        .unwrap();
        hardware.join().unwrap();
        assert_eq!((i.phys_addr(), q.phys_addr()), (0x1000_0000, 0x1100_0000));
        assert_eq!(be.peek(GENERATOR), 4096);
        assert_eq!(be.peek(GENERATOR + 4), 0, "lowered afterwards");
        let writes: Vec<(u64, u32)> = be
            .log()
            .iter()
            .filter_map(|a| match a {
                MockAccess::Write { addr, value } => Some((*addr, *value)),
                _ => None,
            })
            .collect();
        let at = |w: (u64, u32)| writes.iter().position(|&x| x == w).unwrap();
        let sent = at((GENERATOR + 4, 1));
        for dma in [I_DMA, Q_DMA] {
            assert!(
                at((dma + 0x58, 65536)) < sent,
                "length starts the transfer: before the packet"
            );
        }
    }

    /// Two DMAs and a packet generator, with hardware that hands `old` stale packets to the I engine before the
    /// request, and completes both engines after it, the I engine having received `i_bytes`.
    fn with_hardware(old: usize, i_bytes: u32) -> (MockBackend, Result<(), RadioError>) {
        let be = MockBackend::new();
        for dma in [I_DMA, Q_DMA] {
            be.poke(dma + 0x34, 1);
            be.set_write1_clear(dma + 0x34);
        }
        let map = |at| be.map(at, 0x1000).unwrap();
        let mut generator = PacketGenerator::new(map(GENERATOR));
        let i_dma = AxiDma::with_length_width(map(I_DMA), 26);
        let q_dma = AxiDma::with_length_width(map(Q_DMA), 26);
        let done = Arc::new(AtomicBool::new(false));
        let hardware = {
            let (be, done) = (be.clone(), done.clone());
            std::thread::spawn(move || {
                let armed = |be: &MockBackend| {
                    be.log()
                        .iter()
                        .filter(|a| {
                            matches!(a, MockAccess::Write { addr, .. } if *addr == I_DMA + 0x58)
                        })
                        .count()
                };
                for n in 1..=old {
                    while armed(&be) < n && !done.load(Ordering::Relaxed) {
                        std::thread::yield_now();
                    }
                    be.poke(I_DMA + 0x34, (1 << 12) | 2);
                }
                while be.peek(GENERATOR + 4) != 1 {
                    if done.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::yield_now();
                }
                be.poke(I_DMA + 0x58, i_bytes);
                for dma in [I_DMA, Q_DMA] {
                    be.poke(dma + 0x34, (1 << 12) | 2);
                }
            })
        };
        let buffers = (MockBuffer::new(64, 0x1000), MockBuffer::new(64, 0x2000));
        // A thread of this computer answers far more slowly than data already waiting in the hardware.
        let result = record_after(
            &mut generator,
            &i_dma,
            &q_dma,
            buffers,
            32,
            Duration::from_secs(5),
            Duration::from_millis(200),
        )
        .map(|_| ());
        done.store(true, Ordering::Relaxed);
        hardware.join().unwrap();
        (be, result)
    }

    #[test]
    fn data_left_over_is_discarded_before_the_packet_is_asked_for() {
        let (be, result) = with_hardware(3, 64);
        result.unwrap();
        let log = be.log();
        let position = |addr: u64, value: u32| {
            log.iter()
                .enumerate()
                .filter(|(_, a)| matches!(a, MockAccess::Write { addr: at, value: v } if (*at, *v) == (addr, value)))
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
        };
        let arms = position(I_DMA + 0x58, 64);
        assert_eq!(
            arms.len(),
            4,
            "once, and again after each of three stale packets"
        );
        assert!(*arms.last().unwrap() < position(GENERATOR + 4, 1)[0]);
    }

    #[test]
    fn a_record_that_arrives_short_is_an_error() {
        let (_be, result) = with_hardware(0, 48);
        assert!(
            matches!(
                result,
                Err(RadioError::Short {
                    got: 48,
                    wanted: 64
                })
            ),
            "{result:?}"
        );
    }

    #[test]
    fn impossible_lengths_are_refused_before_anything_is_written() {
        let be = MockBackend::new();
        let map = |at| be.map(at, 0x1000).unwrap();
        let mut generator = PacketGenerator::new(map(GENERATOR));
        let i_dma = AxiDma::with_length_width(map(I_DMA), 26);
        let q_dma = AxiDma::with_length_width(map(Q_DMA), 26);
        for samples in [0, 8, 17, 32776] {
            let buffers = (MockBuffer::new(64, 0), MockBuffer::new(64, 64));
            let result = record(
                &mut generator,
                &i_dma,
                &q_dma,
                buffers,
                samples,
                Duration::ZERO,
            );
            assert!(matches!(result, Err(RadioError::RecordLength(n)) if n == samples));
        }
        assert!(matches!(
            generator.set_length(1),
            Err(RadioError::PacketLength(1))
        ));
        assert!(matches!(
            generator.set_length(4097),
            Err(RadioError::PacketLength(4097))
        ));
        assert!(be.log().is_empty());
    }
}
