//! The generic interrupt controller (ARM PL390 / GIC v1) of the Cortex-A9 MPCore.
//!
//! Two register windows: the distributor (`0xF8F0_1000`), which routes interrupts, and the
//! per-CPU interface (`0xF8F0_0100`), which a core uses to acknowledge them. Priorities
//! are numerically **lower = more urgent**; the hardware implements the top five bits
//! (steps of 8). This driver is generic over [`Mmio`], so its register effects are
//! unit-tested on the host.

use crate::map::GIC_IRQS;
use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

// Distributor.
const ICDDCR: usize = 0x000;
const ICDISER: usize = 0x100;
const ICDICER: usize = 0x180;
const ICDISPR: usize = 0x200;
const ICDICPR: usize = 0x280;
const ICDIPR: usize = 0x400;
const ICDIPTR: usize = 0x800;
const ICDICFR: usize = 0xC00;
const ICDSGIR: usize = 0xF00;
// CPU interface.
const ICCICR: usize = 0x00;
const ICCPMR: usize = 0x04;
const ICCBPR: usize = 0x08;
const ICCIAR: usize = 0x0C;
const ICCEOIR: usize = 0x10;

/// Reading `ICCIAR` returns this ID when nothing is pending.
pub const SPURIOUS: u32 = 1023;

/// How an interrupt signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Active-high level (`ICDICFR` bits `0b01`).
    Level,
    /// Rising edge (`0b11`).
    Edge,
}

pub struct Gic<R: Mmio> {
    dist: R,
    cpu: R,
}

impl<R: Mmio> Gic<R> {
    pub fn new(dist: R, cpu: R) -> Self {
        Self { dist, cpu }
    }

    /// Bring the controller to a known state: every shared interrupt disabled, routed to
    /// CPU 0, at a middling priority, level-triggered; the distributor and this CPU's
    /// interface enabled; the priority mask open to every priority.
    pub fn init(&self) -> Result<()> {
        self.dist.write32(ICDDCR, 0)?;
        self.cpu.write32(ICCICR, 0)?;
        for w in 0..GIC_IRQS / 32 {
            self.dist.write32(ICDICER + 4 * w, !0)?; // disable all
            self.dist.write32(ICDICPR + 4 * w, !0)?; // clear pending
        }
        for w in 0..GIC_IRQS / 4 {
            self.dist.write32(ICDIPR + 4 * w, 0xA0A0_A0A0)?;
        }
        // Targets of IDs 0..32 are read-only (banked per CPU); route the shared ones to CPU 0.
        for w in 8..GIC_IRQS / 4 {
            self.dist.write32(ICDIPTR + 4 * w, 0x0101_0101)?;
        }
        // Shared interrupts level-triggered, active high (N-N model, `0b01` per interrupt).
        for w in 2..GIC_IRQS / 16 {
            self.dist.write32(ICDICFR + 4 * w, 0x5555_5555)?;
        }
        self.dist.write32(ICDDCR, 1)?;
        self.cpu.write32(ICCPMR, 0xF8)?; // every priority passes (hardware keeps bits 7:3)
        self.cpu.write32(ICCBPR, 0)?;
        self.cpu.write32(ICCICR, 0x7)?; // enable; AckCtl; secure and non-secure groups
        Ok(())
    }

    fn check(id: u32) -> Result<()> {
        if (id as usize) < GIC_IRQS {
            Ok(())
        } else {
            Err(MmioError::OutOfBounds {
                offset: id as usize,
                len: GIC_IRQS,
            })
        }
    }

    pub fn enable(&self, id: u32) -> Result<()> {
        Self::check(id)?;
        self.dist
            .write32(ICDISER + 4 * (id as usize / 32), 1 << (id % 32))
    }

    pub fn disable(&self, id: u32) -> Result<()> {
        Self::check(id)?;
        self.dist
            .write32(ICDICER + 4 * (id as usize / 32), 1 << (id % 32))
    }

    /// `priority` 0 is the most urgent; only bits 7:3 take effect.
    pub fn set_priority(&self, id: u32, priority: u8) -> Result<()> {
        Self::check(id)?;
        self.set_byte(ICDIPR, id, priority)
    }

    /// Route a shared interrupt (ID >= 32) to the CPUs in `cpu_mask` (bit 0 = CPU 0).
    pub fn set_target(&self, id: u32, cpu_mask: u8) -> Result<()> {
        Self::check(id)?;
        self.set_byte(ICDIPTR, id, cpu_mask)
    }

    /// Select level or edge triggering for a shared interrupt (ID >= 32); the type of
    /// private interrupts (< 32) is fixed.
    pub fn set_trigger(&self, id: u32, t: Trigger) -> Result<()> {
        Self::check(id)?;
        let off = ICDICFR + 4 * (id as usize / 16);
        let shift = 2 * (id % 16);
        let bits = match t {
            Trigger::Level => 0b01,
            Trigger::Edge => 0b11,
        };
        self.dist
            .modify32(off, |v| (v & !(0b11 << shift)) | (bits << shift))
    }

    pub fn is_pending(&self, id: u32) -> Result<bool> {
        Self::check(id)?;
        Ok(self.dist.read32(ICDISPR + 4 * (id as usize / 32))? & (1 << (id % 32)) != 0)
    }

    pub fn clear_pending(&self, id: u32) -> Result<()> {
        Self::check(id)?;
        self.dist
            .write32(ICDICPR + 4 * (id as usize / 32), 1 << (id % 32))
    }

    /// Raise software-generated interrupt `sgi` (0..16) on the CPUs in `cpu_mask`.
    pub fn send_sgi(&self, sgi: u32, cpu_mask: u8) -> Result<()> {
        self.dist
            .write32(ICDSGIR, (u32::from(cpu_mask) << 16) | (sgi & 0xF))
    }

    /// Only interrupts more urgent than `mask` reach this CPU (`0xFF`/`0xF8` passes all).
    /// The RTOS kernel uses this for critical sections that leave urgent interrupts live.
    pub fn set_priority_mask(&self, mask: u8) -> Result<()> {
        self.cpu.write32(ICCPMR, u32::from(mask))
    }

    pub fn priority_mask(&self) -> Result<u8> {
        Ok(self.cpu.read32(ICCPMR)? as u8)
    }

    /// Acknowledge the highest-priority pending interrupt; `None` if it was spurious.
    pub fn acknowledge(&self) -> Result<Option<u32>> {
        let id = self.cpu.read32(ICCIAR)? & 0x3FF;
        Ok((id != SPURIOUS).then_some(id))
    }

    /// Signal that handling of `id` is finished.
    pub fn end_of_interrupt(&self, id: u32) -> Result<()> {
        self.cpu.write32(ICCEOIR, id)
    }

    /// Read-modify-write one byte lane of a byte-per-interrupt register array.
    fn set_byte(&self, base: usize, id: u32, value: u8) -> Result<()> {
        let off = base + (id as usize & !3);
        let shift = 8 * (id % 4);
        self.dist.modify32(off, |v| {
            (v & !(0xFF << shift)) | (u32::from(value) << shift)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const D: u64 = 0xF8F0_1000;
    const C: u64 = 0xF8F0_0100;

    fn gic() -> (MockBackend, Gic<hardroxide_mmio::MockRegion>) {
        let be = MockBackend::new();
        let g = Gic::new(be.map(D, 0x1000).unwrap(), be.map(C, 0x100).unwrap());
        (be, g)
    }

    #[test]
    fn init_routes_and_enables() {
        let (be, g) = gic();
        g.init().unwrap();
        assert_eq!(be.peek(D + ICDDCR as u64), 1);
        assert_eq!(be.peek(C + ICCICR as u64), 7);
        assert_eq!(be.peek(C + ICCPMR as u64), 0xF8);
        // SPI 61 (IRQ_F2P[0]) is routed to CPU 0 at the default priority, level triggered.
        assert_eq!(be.peek(D + ICDIPTR as u64 + 60), 0x0101_0101);
        assert_eq!(be.peek(D + ICDIPR as u64 + 60), 0xA0A0_A0A0);
        assert_eq!(be.peek(D + ICDICFR as u64 + 4 * (61 / 16)), 0x5555_5555);
        // Everything is disabled and nothing pending.
        for w in 0..3 {
            assert_eq!(be.peek(D + ICDICER as u64 + 4 * w), !0);
            assert_eq!(be.peek(D + ICDICPR as u64 + 4 * w), !0);
        }
    }

    #[test]
    fn enable_disable_use_the_set_and_clear_registers() {
        let (be, g) = gic();
        g.enable(61).unwrap();
        assert_eq!(be.peek(D + ICDISER as u64 + 4), 1 << (61 - 32));
        g.disable(29).unwrap();
        assert_eq!(be.peek(D + ICDICER as u64), 1 << 29);
        assert!(g.enable(96).is_err());
    }

    #[test]
    fn priority_and_target_are_byte_lanes() {
        let (be, g) = gic();
        be.poke(D + ICDIPR as u64 + 60, 0xAABB_CCDD);
        g.set_priority(61, 0x10).unwrap(); // byte 1 of the word holding IDs 60..=63
        assert_eq!(be.peek(D + ICDIPR as u64 + 60), 0xAABB_10DD);
        be.poke(D + ICDIPTR as u64 + 60, 0x0101_0101);
        g.set_target(63, 0x02).unwrap();
        assert_eq!(be.peek(D + ICDIPTR as u64 + 60), 0x0201_0101);
    }

    #[test]
    fn trigger_sets_two_bits_per_interrupt() {
        let (be, g) = gic();
        be.poke(D + ICDICFR as u64 + 4 * (61 / 16), 0x5555_5555);
        g.set_trigger(61, Trigger::Edge).unwrap(); // 61 % 16 = 13 -> bits 27:26
        assert_eq!(
            be.peek(D + ICDICFR as u64 + 4 * (61 / 16)),
            0x5555_5555 | (0b10 << 26)
        );
        g.set_trigger(61, Trigger::Level).unwrap();
        assert_eq!(be.peek(D + ICDICFR as u64 + 4 * (61 / 16)), 0x5555_5555);
    }

    #[test]
    fn acknowledge_and_end_of_interrupt() {
        let (be, g) = gic();
        be.poke(C + ICCIAR as u64, 61);
        assert_eq!(g.acknowledge().unwrap(), Some(61));
        g.end_of_interrupt(61).unwrap();
        assert_eq!(be.peek(C + ICCEOIR as u64), 61);
        be.poke(C + ICCIAR as u64, 1023);
        assert_eq!(g.acknowledge().unwrap(), None);
        // The CPU ID lives in bits 12:10 of ICCIAR for SGIs; it is masked off.
        be.poke(C + ICCIAR as u64, (2 << 10) | 5);
        assert_eq!(g.acknowledge().unwrap(), Some(5));
    }

    #[test]
    fn software_interrupt_targets_the_given_cpus() {
        let (be, g) = gic();
        g.send_sgi(3, 0b01).unwrap();
        assert_eq!(be.peek(D + ICDSGIR as u64), (1 << 16) | 3);
    }
}
