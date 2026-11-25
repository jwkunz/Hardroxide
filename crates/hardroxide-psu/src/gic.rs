//! The generic interrupt controller (GIC-400, GICv2) of the Zynq UltraScale+ APU.
//!
//! Two register windows: the distributor (`0xF901_0000`), which routes interrupts, and the
//! per-CPU interface (`0xF902_0000`), which a core uses to acknowledge them. Priorities
//! are numerically **lower = more urgent**; the hardware implements the top five bits
//! (steps of 8). This driver is generic over [`Mmio`], so its register effects are
//! unit-tested on the host. It is the Zynq-7000 driver's interface over 192 interrupt IDs.
//!
//! The pending registers do not show a level-triggered line while the distributor is
//! disabled (seen on hardware); the line itself reads from [`Gic::line_status`].

use crate::map::GIC_IRQS;
use hardroxide_mmio::{Mmio, MmioError};

pub type Result<T> = core::result::Result<T, MmioError>;

// Distributor.
const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_ISPENDR: usize = 0x200;
const GICD_ICPENDR: usize = 0x280;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;
const GICD_ICFGR: usize = 0xC00;
const GICD_SPISR: usize = 0xD04;
const GICD_SGIR: usize = 0xF00;
// CPU interface.
const GICC_CTLR: usize = 0x00;
const GICC_PMR: usize = 0x04;
const GICC_BPR: usize = 0x08;
const GICC_IAR: usize = 0x0C;
const GICC_EOIR: usize = 0x10;

/// Reading `GICC_IAR` returns this ID when nothing is pending.
pub const SPURIOUS: u32 = 1023;

/// How an interrupt signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Active-high level (`GICD_ICFGR` bits `0b01`).
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
        self.dist.write32(GICD_CTLR, 0)?;
        self.cpu.write32(GICC_CTLR, 0)?;
        for w in 0..GIC_IRQS / 32 {
            self.dist.write32(GICD_ICENABLER + 4 * w, !0)?; // disable all
            self.dist.write32(GICD_ICPENDR + 4 * w, !0)?; // clear pending
        }
        for w in 0..GIC_IRQS / 4 {
            self.dist.write32(GICD_IPRIORITYR + 4 * w, 0xA0A0_A0A0)?;
        }
        // Targets of IDs 0..32 are read-only (banked per CPU); route the shared ones to CPU 0.
        for w in 8..GIC_IRQS / 4 {
            self.dist.write32(GICD_ITARGETSR + 4 * w, 0x0101_0101)?;
        }
        // Shared interrupts level-triggered, active high.
        for w in 2..GIC_IRQS / 16 {
            self.dist.write32(GICD_ICFGR + 4 * w, 0x5555_5555)?;
        }
        self.dist.write32(GICD_CTLR, 1)?;
        self.cpu.write32(GICC_PMR, 0xF8)?; // every priority passes (hardware keeps bits 7:3)
        self.cpu.write32(GICC_BPR, 0)?;
        // Secure view: enable both groups, acknowledge either, signal group 0 as IRQ.
        self.cpu.write32(GICC_CTLR, 0x7)?;
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
            .write32(GICD_ISENABLER + 4 * (id as usize / 32), 1 << (id % 32))
    }

    pub fn disable(&self, id: u32) -> Result<()> {
        Self::check(id)?;
        self.dist
            .write32(GICD_ICENABLER + 4 * (id as usize / 32), 1 << (id % 32))
    }

    /// `priority` 0 is the most urgent; only bits 7:3 take effect.
    pub fn set_priority(&self, id: u32, priority: u8) -> Result<()> {
        Self::check(id)?;
        self.set_byte(GICD_IPRIORITYR, id, priority)
    }

    /// Route a shared interrupt (ID >= 32) to the CPUs in `cpu_mask` (bit 0 = CPU 0).
    pub fn set_target(&self, id: u32, cpu_mask: u8) -> Result<()> {
        Self::check(id)?;
        self.set_byte(GICD_ITARGETSR, id, cpu_mask)
    }

    /// Select level or edge triggering for a shared interrupt (ID >= 32); the type of
    /// private interrupts (< 32) is fixed.
    pub fn set_trigger(&self, id: u32, t: Trigger) -> Result<()> {
        Self::check(id)?;
        let off = GICD_ICFGR + 4 * (id as usize / 16);
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
        Ok(self.dist.read32(GICD_ISPENDR + 4 * (id as usize / 32))? & (1 << (id % 32)) != 0)
    }

    /// The level of shared interrupt `id`'s input line (GIC-400 `GICD_SPISRn`), whether or
    /// not the controller is enabled. `id` must be at least 32.
    pub fn line_status(&self, id: u32) -> Result<bool> {
        Self::check(id)?;
        let Some(n) = id.checked_sub(32) else {
            return Err(MmioError::OutOfBounds {
                offset: id as usize,
                len: GIC_IRQS,
            });
        };
        Ok(self.dist.read32(GICD_SPISR + 4 * (n as usize / 32))? & (1 << (n % 32)) != 0)
    }

    pub fn clear_pending(&self, id: u32) -> Result<()> {
        Self::check(id)?;
        self.dist
            .write32(GICD_ICPENDR + 4 * (id as usize / 32), 1 << (id % 32))
    }

    /// Raise software-generated interrupt `sgi` (0..16) on the CPUs in `cpu_mask`.
    pub fn send_sgi(&self, sgi: u32, cpu_mask: u8) -> Result<()> {
        self.dist
            .write32(GICD_SGIR, (u32::from(cpu_mask) << 16) | (sgi & 0xF))
    }

    /// Only interrupts more urgent than `mask` reach this CPU (`0xFF`/`0xF8` passes all).
    /// The RTOS kernel uses this for critical sections that leave urgent interrupts live.
    pub fn set_priority_mask(&self, mask: u8) -> Result<()> {
        self.cpu.write32(GICC_PMR, u32::from(mask))
    }

    pub fn priority_mask(&self) -> Result<u8> {
        Ok(self.cpu.read32(GICC_PMR)? as u8)
    }

    /// Acknowledge the highest-priority pending interrupt; `None` if it was spurious.
    pub fn acknowledge(&self) -> Result<Option<u32>> {
        let id = self.cpu.read32(GICC_IAR)? & 0x3FF;
        Ok((id != SPURIOUS).then_some(id))
    }

    /// Signal that handling of `id` is finished.
    pub fn end_of_interrupt(&self, id: u32) -> Result<()> {
        self.cpu.write32(GICC_EOIR, id)
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

    const D: u64 = 0xF901_0000;
    const C: u64 = 0xF902_0000;

    fn gic() -> (MockBackend, Gic<hardroxide_mmio::MockRegion>) {
        let be = MockBackend::new();
        let g = Gic::new(be.map(D, 0x1000).unwrap(), be.map(C, 0x2000).unwrap());
        (be, g)
    }

    #[test]
    fn init_covers_all_192_interrupts() {
        let (be, g) = gic();
        g.init().unwrap();
        assert_eq!(be.peek(D + GICD_CTLR as u64), 1);
        assert_eq!(be.peek(C + GICC_CTLR as u64), 7);
        assert_eq!(be.peek(C + GICC_PMR as u64), 0xF8);
        // The last interrupt (191) is disabled, routed to CPU 0, mid priority and level.
        assert_eq!(be.peek(D + GICD_ICENABLER as u64 + 4 * 5), !0);
        assert_eq!(be.peek(D + GICD_ITARGETSR as u64 + 188), 0x0101_0101);
        assert_eq!(be.peek(D + GICD_IPRIORITYR as u64 + 188), 0xA0A0_A0A0);
        assert_eq!(be.peek(D + GICD_ICFGR as u64 + 4 * 11), 0x5555_5555);
        // Nothing past the last register.
        assert_eq!(be.peek(D + GICD_ICENABLER as u64 + 4 * 6), 0);
    }

    #[test]
    fn pl_interrupt_registers() {
        let (be, g) = gic();
        g.enable(121).unwrap();
        assert_eq!(be.peek(D + GICD_ISENABLER as u64 + 12), 1 << 25);
        g.disable(136).unwrap();
        assert_eq!(be.peek(D + GICD_ICENABLER as u64 + 16), 1 << 8);
        assert!(g.enable(192).is_err());
        // ID 121 is bit 25 of the third line-status register, as read on the board.
        assert!(!g.line_status(121).unwrap());
        be.poke(D + GICD_SPISR as u64 + 8, 0x0200_0000);
        assert!(g.line_status(121).unwrap());
        assert!(g.line_status(29).is_err());
    }

    #[test]
    fn priority_target_and_trigger() {
        let (be, g) = gic();
        be.poke(D + GICD_IPRIORITYR as u64 + 120, 0xAABB_CCDD);
        g.set_priority(121, 0x10).unwrap();
        assert_eq!(be.peek(D + GICD_IPRIORITYR as u64 + 120), 0xAABB_10DD);
        be.poke(D + GICD_ITARGETSR as u64 + 120, 0x0101_0101);
        g.set_target(123, 0x02).unwrap();
        assert_eq!(be.peek(D + GICD_ITARGETSR as u64 + 120), 0x0201_0101);
        be.poke(D + GICD_ICFGR as u64 + 4 * (121 / 16), 0x5555_5555);
        g.set_trigger(121, Trigger::Edge).unwrap(); // 121 % 16 = 9 -> bits 19:18
        assert_eq!(
            be.peek(D + GICD_ICFGR as u64 + 4 * (121 / 16)),
            0x5555_5555 | (0b10 << 18)
        );
    }

    #[test]
    fn acknowledge_end_and_software_interrupt() {
        let (be, g) = gic();
        be.poke(C + GICC_IAR as u64, 121);
        assert_eq!(g.acknowledge().unwrap(), Some(121));
        g.end_of_interrupt(121).unwrap();
        assert_eq!(be.peek(C + GICC_EOIR as u64), 121);
        be.poke(C + GICC_IAR as u64, 1023);
        assert_eq!(g.acknowledge().unwrap(), None);
        g.send_sgi(3, 0b01).unwrap();
        assert_eq!(be.peek(D + GICD_SGIR as u64), (1 << 16) | 3);
    }
}
