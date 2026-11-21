//! Waiting for a PL interrupt the way the Linux UIO backend does, so the drivers that take an
//! [`IrqWaiter`] (the interrupt controller, DMA) work unchanged on bare metal. ARM only.
//!
//! The first call to [`IrqWait::new`] puts the interrupt in *event mode* (see [`crate::irq`]):
//! when it fires, the vector entry masks it in the GIC and counts it. [`IrqWaiter::enable`]
//! unmasks it again; [`IrqWaiter::wait_event`] returns the cumulative count when it changes.
//! Interrupts must be unmasked on the core ([`crate::cp15::enable_irq`]) or nothing can
//! arrive; waiting with them masked is reported as an error rather than hanging.

use crate::{cp15, irq, map};
use core::time::Duration;
use hardroxide_irq::{IrqError, IrqLine, IrqWaiter};
use hardroxide_mmio::time::Instant;
use hardroxide_mmio::{MmioBackend, PhysMem};

/// One GIC interrupt, waitable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqWait {
    id: u32,
}

impl IrqWait {
    /// Wait on GIC interrupt `id` (for a PL interrupt see [`map::f2p_irq`]); puts it in
    /// event mode. `None` if `id` is not a valid interrupt.
    pub fn new(id: u32) -> Option<Self> {
        irq::register_event(id).then_some(IrqWait { id })
    }

    /// Wait on a generated interrupt line. `None` if it does not reach the PS directly
    /// (it feeds an interrupt controller, or could not be traced).
    pub fn for_line(line: &IrqLine) -> Option<Self> {
        let spi = line.gic_spi?;
        Self::new(map::SPI_BASE + spi)
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

fn gic() -> Result<crate::gic::Gic<hardroxide_mmio::PhysRegion>, IrqError> {
    // SAFETY: the GIC windows belong to the interrupt layer of this crate.
    let be = unsafe { PhysMem::new() };
    let map = |base: usize, len: usize| {
        be.map(base as u64, len)
            .map_err(|_| IrqError::Platform("cannot map the GIC"))
    };
    Ok(crate::gic::Gic::new(
        map(map::GIC_DIST_BASE, map::GIC_DIST_SIZE)?,
        map(map::GIC_CPU_BASE, map::GIC_CPU_SIZE)?,
    ))
}

impl IrqWaiter for IrqWait {
    fn enable(&self) -> Result<(), IrqError> {
        gic()?
            .enable(self.id)
            .map_err(|_| IrqError::Platform("cannot enable the interrupt"))
    }

    fn wait_event(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError> {
        if !cp15::irq_enabled() {
            return Err(IrqError::Platform(
                "IRQs are masked on this core; waiting for an interrupt would never end",
            ));
        }
        let t0 = Instant::now();
        loop {
            if let Some(n) = irq::take_event(self.id) {
                return Ok(Some(n));
            }
            if timeout.is_some_and(|t| t0.elapsed() >= t) {
                return Ok(None);
            }
            core::hint::spin_loop();
        }
    }
}
