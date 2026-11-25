//! The Zynq UltraScale+ processing-system memory map and interrupt numbers used by this
//! crate (UG1085; Cortex-A53 and GIC-400 TRMs).

/// Clock and reset registers of the low-power domain.
pub const CRL_APB_BASE: usize = 0xFF5E_0000;
pub const CRL_APB_SIZE: usize = 0x1000;
/// Clock and reset registers of the full-power domain.
pub const CRF_APB_BASE: usize = 0xFD1A_0000;
pub const CRF_APB_SIZE: usize = 0x100;

/// UART controllers (Cadence).
pub const UART0_BASE: usize = 0xFF00_0000;
pub const UART1_BASE: usize = 0xFF01_0000;
pub const UART_SIZE: usize = 0x1000;

pub const I2C0_BASE: usize = 0xFF02_0000;
pub const I2C1_BASE: usize = 0xFF03_0000;
pub const I2C_SIZE: usize = 0x1000;

pub const SDIO0_BASE: usize = 0xFF16_0000;
pub const SDIO1_BASE: usize = 0xFF17_0000;
pub const SDIO_SIZE: usize = 0x1000;

pub const GEM0_BASE: usize = 0xFF0B_0000;
pub const GEM1_BASE: usize = 0xFF0C_0000;
pub const GEM2_BASE: usize = 0xFF0D_0000;
pub const GEM3_BASE: usize = 0xFF0E_0000;
pub const GEM_SIZE: usize = 0x1000;

pub const SPI0_BASE: usize = 0xFF04_0000;
pub const SPI1_BASE: usize = 0xFF05_0000;
pub const SPI_SIZE: usize = 0x1000;

/// The GPIO controller of the multiplexed pins ([`crate::gpio`]).
pub const GPIO_BASE: usize = 0xFF0A_0000;
pub const GPIO_SIZE: usize = 0x1000;

/// The system monitors ([`crate::sysmon`]).
pub const SYSMON_BASE: usize = 0xFFA5_0000;
pub const SYSMON_SIZE: usize = 0x1000;

/// The system counter's control registers; it drives the processors' generic timers.
pub const SYSTEM_COUNTER_BASE: usize = 0xFF26_0000;

/// GIC distributor.
pub const GIC_DIST_BASE: usize = 0xF901_0000;
pub const GIC_DIST_SIZE: usize = 0x1000;
/// GIC CPU interface.
pub const GIC_CPU_BASE: usize = 0xF902_0000;
pub const GIC_CPU_SIZE: usize = 0x2000;

/// Number of interrupt IDs the GIC-400 of the Zynq UltraScale+ implements (0..192).
pub const GIC_IRQS: usize = 192;
/// Software-generated interrupts 0..=15, private peripheral interrupts 16..=31,
/// shared peripheral interrupts from 32.
pub const SPI_BASE: u32 = 32;

/// Private peripheral interrupt IDs (the generic timers).
pub mod ppi {
    pub const HYPERVISOR_TIMER: u32 = 26;
    pub const VIRTUAL_TIMER: u32 = 27;
    /// The secure physical timer: the one a program running at EL3 uses.
    pub const SECURE_PHYSICAL_TIMER: u32 = 29;
    pub const PHYSICAL_TIMER: u32 = 30;
}

/// GIC interrupt IDs of shared peripherals this crate knows about.
pub mod spi {
    pub const UART0: u32 = 53;
    pub const UART1: u32 = 54;
    /// PL-to-PS interrupts `pl_ps_irq0[7:0]` are IDs 121..=128.
    pub const PL_PS_0: u32 = 121;
    /// `pl_ps_irq1[7:0]` are IDs 136..=143.
    pub const PL_PS_1: u32 = 136;
}

/// The GIC interrupt ID of PL interrupt `bit`: 0..8 is `pl_ps_irq0`, 8..16 `pl_ps_irq1`.
pub const fn pl_irq(bit: u32) -> u32 {
    if bit < 8 {
        spi::PL_PS_0 + bit
    } else {
        spi::PL_PS_1 + (bit - 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pl_interrupts_map_to_the_ids_seen_on_the_board() {
        // The fixture probes found pl_ps_irq0[0] at ID 121 and pl_ps_irq1[0] at 136.
        assert_eq!(pl_irq(0), 121);
        assert_eq!(pl_irq(7), 128);
        assert_eq!(pl_irq(8), 136);
        assert_eq!(pl_irq(15), 143);
        assert!((pl_irq(15) as usize) < GIC_IRQS);
    }

    #[test]
    fn gic_windows_do_not_overlap() {
        const { assert!(GIC_DIST_BASE + GIC_DIST_SIZE <= GIC_CPU_BASE) };
    }
}
