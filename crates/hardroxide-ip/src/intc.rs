//! AXI Interrupt Controller (`xilinx.com:ip:axi_intc`, PG099) used as a
//! software-driven fan-in: many PL interrupts share the controller's single
//! `irq` output (one UIO device); this driver says which input fired.
//! UNVERIFIED on hardware; offsets/bits are asserted against the HWH.

use hardroxide_irq::{IrqError, IrqWaiter};
use hardroxide_mmio::{Mmio, MmioError};
use std::time::{Duration, Instant};

/// Interrupt status (input latched, before masking).
pub const ISR: usize = 0x00;
/// Interrupt pending (`ISR & IER`).
pub const IPR: usize = 0x04;
/// Interrupt enable.
pub const IER: usize = 0x08;
/// Interrupt acknowledge (write 1 to clear the latched input).
pub const IAR: usize = 0x0C;
/// Set interrupt enables (write 1 to set).
pub const SIE: usize = 0x10;
/// Clear interrupt enables (write 1 to clear).
pub const CIE: usize = 0x14;
/// Interrupt vector: highest-priority pending input, `0xFFFF_FFFF` if none.
pub const IVR: usize = 0x18;
/// Master enable.
pub const MER: usize = 0x1C;

const MER_ME: u32 = 1 << 0;
const MER_HIE: u32 = 1 << 1;

#[derive(Debug, thiserror::Error)]
pub enum IntcError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error(transparent)]
    Irq(#[from] IrqError),
}

pub struct AxiIntc<R: Mmio> {
    regs: R,
}

impl<R: Mmio> AxiIntc<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Disable and clear every input, then turn the controller's `irq`
    /// output on (`MER.ME | MER.HIE`).
    pub fn init(&self) -> Result<(), IntcError> {
        self.regs.write32(IER, 0)?;
        self.regs.write32(IAR, 0xFFFF_FFFF)?;
        self.regs.write32(MER, MER_ME | MER_HIE)?;
        Ok(())
    }

    /// Enable the inputs set in `mask` (bit n = input `In<n>`).
    pub fn enable(&self, mask: u32) -> Result<(), IntcError> {
        Ok(self.regs.write32(SIE, mask)?)
    }

    pub fn disable(&self, mask: u32) -> Result<(), IntcError> {
        Ok(self.regs.write32(CIE, mask)?)
    }

    pub fn enabled(&self) -> Result<u32, IntcError> {
        Ok(self.regs.read32(IER)?)
    }

    /// Inputs that are latched *and* enabled.
    pub fn pending(&self) -> Result<u32, IntcError> {
        Ok(self.regs.read32(IPR)?)
    }

    /// Lowest-numbered (highest-priority) pending input, if any.
    pub fn highest_pending(&self) -> Result<Option<u32>, IntcError> {
        let v = self.regs.read32(IVR)?;
        Ok((v != 0xFFFF_FFFF).then_some(v))
    }

    /// Clear the latched inputs in `mask`. For level-sensitive inputs,
    /// acknowledge the source device first or the input re-latches at once.
    pub fn ack(&self, mask: u32) -> Result<(), IntcError> {
        Ok(self.regs.write32(IAR, mask)?)
    }

    /// Wait until any input in `mask` is pending, using the controller's
    /// UIO interrupt `irq`. Returns the pending bits within `mask`, or
    /// `None` on timeout. Does *not* acknowledge: handle the source, then
    /// call [`ack`](Self::ack).
    pub fn wait<W: IrqWaiter>(
        &self,
        irq: &W,
        mask: u32,
        timeout: Option<Duration>,
    ) -> Result<Option<u32>, IntcError> {
        let t0 = Instant::now();
        loop {
            // Unmask first, then look: an input latched in between wakes us.
            irq.enable()?;
            let p = self.pending()? & mask;
            if p != 0 {
                return Ok(Some(p));
            }
            let left = match timeout {
                None => None,
                Some(t) => match t.checked_sub(t0.elapsed()) {
                    Some(l) => Some(l),
                    None => return Ok(None),
                },
            };
            if irq.wait_event(left)?.is_none() {
                return Ok(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockAccess as Access, MockBackend, MockRegion};
    use std::cell::{Cell, RefCell};

    const BASE: u64 = 0x4180_0000;

    /// Waiter that raises scripted inputs when waited on.
    struct Fake<'a> {
        be: &'a MockBackend,
        script: RefCell<Vec<u32>>, // pending values to set on each wait
        enables: Cell<u32>,
    }

    impl IrqWaiter for Fake<'_> {
        fn enable(&self) -> Result<(), IrqError> {
            self.enables.set(self.enables.get() + 1);
            Ok(())
        }
        fn wait_event(&self, _: Option<Duration>) -> Result<Option<u32>, IrqError> {
            Ok(self.script.borrow_mut().pop().map(|v| {
                self.be.poke(BASE + IPR as u64, v);
                1
            }))
        }
    }

    fn intc() -> (MockBackend, AxiIntc<MockRegion>) {
        let be = MockBackend::new();
        let regs = be.map(BASE, 0x1000).unwrap();
        (be, AxiIntc::new(regs))
    }

    #[test]
    fn init_masks_everything_and_enables_output() {
        let (be, ic) = intc();
        ic.init().unwrap();
        let log = be.log();
        assert!(log.iter().any(|a| *a
            == Access::Write {
                addr: BASE + IER as u64,
                value: 0
            }));
        assert_eq!(be.peek(BASE + MER as u64), MER_ME | MER_HIE);
        ic.enable(0b101).unwrap();
        assert_eq!(be.peek(BASE + SIE as u64), 0b101);
        ic.disable(0b001).unwrap();
        assert_eq!(be.peek(BASE + CIE as u64), 0b001);
        ic.ack(0b100).unwrap();
        assert_eq!(be.peek(BASE + IAR as u64), 0b100);
    }

    #[test]
    fn wait_filters_by_mask_and_times_out() {
        let (be, ic) = intc();
        // First wake is input 0 (not wanted), second is input 2.
        let w = Fake {
            be: &be,
            script: RefCell::new(vec![0b100, 0b001]),
            enables: Cell::new(0),
        };
        assert_eq!(
            ic.wait(&w, 0b100, Some(Duration::from_secs(1))).unwrap(),
            Some(0b100)
        );
        assert_eq!(w.enables.get(), 3); // re-armed before every look
        // Nothing left: wait_event reports a timeout.
        be.poke(BASE + IPR as u64, 0);
        assert_eq!(
            ic.wait(&w, 0b100, Some(Duration::from_secs(1))).unwrap(),
            None
        );
    }

    #[test]
    fn registers_match_the_hwh() {
        let h = |r, f| crate::hwh("pynq_z2_intc", "axi_intc_0", r, f);
        for (r, o) in [
            ("ISR", ISR),
            ("IPR", IPR),
            ("IER", IER),
            ("IAR", IAR),
            ("SIE", SIE),
            ("CIE", CIE),
            ("IVR", IVR),
            ("MER", MER),
        ] {
            assert_eq!(h(r, None).0, o, "{r}");
        }
        assert_eq!(1u32 << h("MER", Some("ME")).1, MER_ME);
        assert_eq!(1u32 << h("MER", Some("HIE")).1, MER_HIE);
    }
}
