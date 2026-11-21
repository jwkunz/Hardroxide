//! Video Timing Controller (`xilinx.com:ip:v_tc`, PG016), generator side: produces the blanking,
//! sync and active-video signals of a display mode. The register layout and the way a mode is
//! programmed follow the vendor driver's generator interface (`XVtc_SetGenerator`).

use hardroxide_mmio::{Mmio, MmioError};

const CTL: usize = 0x000;
const GASIZE: usize = 0x060;
const GPOL: usize = 0x06C;
const GHSIZE: usize = 0x070;
const GVSIZE: usize = 0x074;
const GHSYNC: usize = 0x078;
const GVBHOFF: usize = 0x07C;
const GVSYNC: usize = 0x080;
const GVSHOFF: usize = 0x084;
const GVBHOFF_F1: usize = 0x088;
const GVSYNC_F1: usize = 0x08C;
const GVSHOFF_F1: usize = 0x090;
const GASIZE_F1: usize = 0x094;

const CTL_SW: u32 = 1 << 0; // core enable
const CTL_RU: u32 = 1 << 1; // register update
const CTL_GE: u32 = 1 << 2; // generator enable
const CTL_RESET: u32 = 1 << 31;

const POL_VBP: u32 = 1 << 0;
const POL_HBP: u32 = 1 << 1;
const POL_VSP: u32 = 1 << 2;
const POL_HSP: u32 = 1 << 3;
const POL_AVP: u32 = 1 << 4;

/// A progressive display mode as horizontal and vertical timings, in pixels and lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub h_active: u32,
    pub h_front_porch: u32,
    pub h_sync: u32,
    pub h_back_porch: u32,
    pub v_active: u32,
    pub v_front_porch: u32,
    pub v_sync: u32,
    pub v_back_porch: u32,
    /// Horizontal sync is active high.
    pub h_sync_positive: bool,
    /// Vertical sync is active high.
    pub v_sync_positive: bool,
}

impl Timing {
    pub const fn h_total(&self) -> u32 {
        self.h_active + self.h_front_porch + self.h_sync + self.h_back_porch
    }
    pub const fn v_total(&self) -> u32 {
        self.v_active + self.v_front_porch + self.v_sync + self.v_back_porch
    }
}

pub struct VideoTimingController<R: Mmio> {
    regs: R,
}

fn pack(lo: u32, hi: u32) -> u32 {
    (lo & 0x1FFF) | (hi & 0x1FFF) << 16
}

impl<R: Mmio> VideoTimingController<R> {
    pub fn new(regs: R) -> Self {
        Self { regs }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// Reset the core (the reset bit clears itself).
    pub fn reset(&self) -> Result<(), MmioError> {
        self.regs.write32(CTL, CTL_RESET)
    }

    /// Switch the generator off.
    pub fn stop(&self) -> Result<(), MmioError> {
        self.regs.write32(CTL, 0)
    }

    /// Program the generator for `t` and start it. The generator's timing comes from these
    /// registers (not from the detector), and the new values take effect at the next frame.
    pub fn start(&self, t: &Timing) -> Result<(), MmioError> {
        let r = &self.regs;
        let hsync_start = t.h_active + t.h_front_porch;
        let hsync_end = hsync_start + t.h_sync;
        let vsync_start = t.v_active + t.v_front_porch;
        let vsync_end = vsync_start + t.v_sync;
        r.write32(GHSIZE, t.h_total())?;
        r.write32(GVSIZE, pack(t.v_total(), t.v_total()))?;
        r.write32(GASIZE, pack(t.h_active, t.v_active))?;
        r.write32(GASIZE_F1, pack(0, t.v_active))?;
        r.write32(GHSYNC, pack(hsync_start, hsync_end))?;
        r.write32(GVSYNC, pack(vsync_start, vsync_end))?;
        r.write32(GVSYNC_F1, pack(vsync_start, vsync_end))?;
        // The vertical blank and vertical sync begin at fixed horizontal positions.
        r.write32(GVBHOFF, pack(t.h_active, t.h_active))?;
        r.write32(GVBHOFF_F1, pack(t.h_active, t.h_active))?;
        r.write32(GVSHOFF, pack(hsync_start, hsync_start))?;
        r.write32(GVSHOFF_F1, pack(hsync_start, hsync_start))?;
        let mut pol = POL_AVP | POL_HBP | POL_VBP;
        if t.h_sync_positive {
            pol |= POL_HSP;
        }
        if t.v_sync_positive {
            pol |= POL_VSP;
        }
        r.write32(GPOL, pol)?;
        r.write32(CTL, CTL_SW | CTL_RU | CTL_GE)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend};

    const BASE: u64 = 0x43C2_0000;
    /// CEA 1280x720 at 60 Hz: total 1650 x 750, syncs positive.
    const HD: Timing = Timing {
        h_active: 1280,
        h_front_porch: 110,
        h_sync: 40,
        h_back_porch: 220,
        v_active: 720,
        v_front_porch: 5,
        v_sync: 5,
        v_back_porch: 20,
        h_sync_positive: true,
        v_sync_positive: true,
    };

    #[test]
    fn totals_add_up() {
        assert_eq!((HD.h_total(), HD.v_total()), (1650, 750));
    }

    #[test]
    fn start_programs_the_generator_registers() {
        let be = MockBackend::new();
        let v = VideoTimingController::new(be.map(BASE, 0x1000).unwrap());
        v.start(&HD).unwrap();
        let p = |off: usize| be.peek(BASE + off as u64);
        assert_eq!(p(GHSIZE), 1650);
        assert_eq!(p(GVSIZE), 750 | 750 << 16);
        assert_eq!(p(GASIZE), 1280 | 720 << 16);
        assert_eq!(p(GHSYNC), 1390 | 1430 << 16);
        assert_eq!(p(GVSYNC), 725 | 730 << 16);
        assert_eq!(p(GVSHOFF), 1390 | 1390 << 16);
        assert_eq!(p(GPOL), POL_AVP | POL_HBP | POL_VBP | POL_HSP | POL_VSP);
        assert_eq!(p(CTL), CTL_SW | CTL_RU | CTL_GE);
    }

    #[test]
    fn negative_syncs_leave_the_polarity_bits_clear() {
        let be = MockBackend::new();
        let v = VideoTimingController::new(be.map(BASE, 0x1000).unwrap());
        let vga = Timing {
            h_active: 640,
            h_front_porch: 16,
            h_sync: 96,
            h_back_porch: 48,
            v_active: 480,
            v_front_porch: 10,
            v_sync: 2,
            v_back_porch: 33,
            h_sync_positive: false,
            v_sync_positive: false,
        };
        v.start(&vga).unwrap();
        assert_eq!(be.peek(BASE + GPOL as u64), POL_AVP | POL_HBP | POL_VBP);
        assert_eq!(be.peek(BASE + GHSIZE as u64), 800);
    }
}
