//! The Zynq-7000 processing-system memory map and interrupt numbers used by this crate
//! (UG585, Zynq-7000 TRM; Cortex-A9 MPCore TRM for the private peripherals).

/// SLCR (system level control registers).
pub const SLCR_BASE: usize = 0xF800_0000;
pub const SLCR_SIZE: usize = 0x1000;
/// UART controllers (Cadence).
pub const UART0_BASE: usize = 0xE000_0000;
pub const UART1_BASE: usize = 0xE000_1000;
pub const UART_SIZE: usize = 0x1000;

/// Base of the Cortex-A9 MPCore private peripherals (`PERIPHBASE`).
pub const PERIPH_BASE: usize = 0xF8F0_0000;
/// Snoop control unit.
pub const SCU_BASE: usize = PERIPH_BASE;
/// GIC CPU interface.
pub const GIC_CPU_BASE: usize = PERIPH_BASE + 0x100;
pub const GIC_CPU_SIZE: usize = 0x100;
/// Global timer (64-bit, shared by both cores).
pub const GLOBAL_TIMER_BASE: usize = PERIPH_BASE + 0x200;
pub const GLOBAL_TIMER_SIZE: usize = 0x20;
/// Private timer and watchdog (one set per core).
pub const PRIVATE_TIMER_BASE: usize = PERIPH_BASE + 0x600;
pub const PRIVATE_TIMER_SIZE: usize = 0x20;
/// GIC distributor.
pub const GIC_DIST_BASE: usize = PERIPH_BASE + 0x1000;
pub const GIC_DIST_SIZE: usize = 0x1000;
/// PL310 L2 cache controller.
pub const L2_BASE: usize = PERIPH_BASE + 0x2000;

/// Number of interrupt IDs the Zynq-7000 GIC implements (0..96).
pub const GIC_IRQS: usize = 96;
/// Software-generated interrupts 0..=15, private peripheral interrupts 16..=31,
/// shared peripheral interrupts from 32.
pub const SPI_BASE: u32 = 32;

/// Private peripheral interrupt IDs.
pub mod ppi {
    pub const GLOBAL_TIMER: u32 = 27;
    pub const NFIQ: u32 = 28;
    pub const PRIVATE_TIMER: u32 = 29;
    pub const WATCHDOG: u32 = 30;
    pub const NIRQ: u32 = 31;
}

/// GIC interrupt IDs of shared peripherals this crate knows about.
pub mod spi {
    pub const UART0: u32 = 59;
    pub const UART1: u32 = 82;
    /// PL-to-PS interrupts `IRQ_F2P[7:0]` are IDs 61..=68.
    pub const F2P_0_TO_7: u32 = 61;
    /// `IRQ_F2P[15:8]` are IDs 84..=91.
    pub const F2P_8_TO_15: u32 = 84;
}

/// The GIC interrupt ID of PL interrupt `IRQ_F2P[bit]` (`bit` in 0..16).
pub const fn f2p_irq(bit: u32) -> u32 {
    if bit < 8 {
        spi::F2P_0_TO_7 + bit
    } else {
        spi::F2P_8_TO_15 + (bit - 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pl_interrupts_map_to_the_documented_gic_ids() {
        // Device-tree SPI cells 29..=36 and 52..=59 plus 32 (see the generated overlays).
        assert_eq!(f2p_irq(0), 32 + 29);
        assert_eq!(f2p_irq(7), 32 + 36);
        assert_eq!(f2p_irq(8), 32 + 52);
        assert_eq!(f2p_irq(15), 32 + 59);
    }

    #[test]
    fn windows_do_not_overlap() {
        let w = [
            (GIC_CPU_BASE, GIC_CPU_SIZE),
            (GLOBAL_TIMER_BASE, GLOBAL_TIMER_SIZE),
            (PRIVATE_TIMER_BASE, PRIVATE_TIMER_SIZE),
            (GIC_DIST_BASE, GIC_DIST_SIZE),
        ];
        for (i, a) in w.iter().enumerate() {
            for b in &w[i + 1..] {
                assert!(a.0 + a.1 <= b.0 || b.0 + b.1 <= a.0, "{a:?} overlaps {b:?}");
            }
        }
    }
}
