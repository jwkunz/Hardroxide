//! What differs between the processor families, for the programs that run on both (the kernel's test and its jitter
//! benchmark). Each program includes this file as a module and names the family's support crate `ps`.

use hardroxide_mmio::{MmioBackend, PhysMem, PhysRegion};

#[cfg(target_arch = "arm")]
pub use hardroxide_ps7 as ps;
#[cfg(target_arch = "aarch64")]
pub use hardroxide_psu as ps;

pub struct Board {
    /// The clock of the kernel's tick timer: what `hardroxide_rt::start` wants.
    pub tick_clock_hz: u64,
    /// The interrupt controller, initialised.
    pub gic: ps::gic::Gic<PhysRegion>,
}

/// A new handle on the interrupt controller.
pub fn gic() -> ps::gic::Gic<PhysRegion> {
    // SAFETY: the controller's windows; the programs use them for software interrupts and enables only.
    let be = unsafe { PhysMem::new() };
    ps::gic::Gic::new(
        be.map(ps::map::GIC_DIST_BASE as u64, ps::map::GIC_DIST_SIZE)
            .unwrap(),
        be.map(ps::map::GIC_CPU_BASE as u64, ps::map::GIC_CPU_SIZE)
            .unwrap(),
    )
}

/// Console, caches, time source and interrupt controller; prints `title` first.
pub fn init(title: &str) -> Board {
    let tick_clock_hz = family::console_and_memory();
    ps::println!("\n== {title} ==");
    // SAFETY: early in start-up; nothing else touches the time source's timer.
    unsafe { ps::time::init(family::time_clock(tick_clock_hz)) };
    let gic = gic();
    gic.init().unwrap();
    Board { tick_clock_hz, gic }
}

/// Zynq-7000: the PYNQ-Z2, or QEMU's `xilinx-zynq-a9` with `--features qemu`.
#[cfg(target_arch = "arm")]
mod family {
    use super::ps::{clocks, console, map, mem};
    use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};

    const PS_CLK_HZ: u64 = if cfg!(feature = "qemu") {
        33_333_333
    } else {
        50_000_000
    };

    /// What `ps7_init` (or, here, a stand-in) leaves for the program: PLLs running and the UART's clocks and pins.
    fn ps_setup() {
        // SAFETY: this program owns the SLCR for the duration of this setup.
        let be = unsafe { PhysMem::new() };
        let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
        let w = |o: usize, v: u32| slcr.write32(o, v).unwrap();
        let r = |o: usize| slcr.read32(o).unwrap();
        w(0x008, 0xDF0D);
        if cfg!(feature = "qemu") {
            let m = |a: usize, mask: u32, v: u32| w(a, (r(a) & !mask) | (v & mask));
            m(0x108, 0x0007_F000, 0x0002_0000);
            m(0x108, 0x10, 0x10);
            m(0x108, 0x1, 0x1);
            m(0x108, 0x1, 0x0);
            m(0x108, 0x10, 0x0);
        }
        w(0x12C, r(0x12C) | (1 << 20));
        let io_fdiv = (r(0x108) >> 12) & 0x7F;
        w(0x154, ((io_fdiv / 2).max(1) << 8) | 0x3);
        w(0x228, 0);
        w(0x738, 0x16E1);
        w(0x73C, 0x16E0);
        w(0x004, 0x767B);
    }

    pub fn console_and_memory() -> u64 {
        ps_setup();
        // SAFETY: early in start-up, running from DDR at its physical address.
        unsafe { mem::enable(512 << 20) };
        // SAFETY: the SLCR is only read here; the console UART is used by nothing else.
        let be = unsafe { PhysMem::new() };
        let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
        let uart_ref = clocks::uart_ref_hz(&slcr, PS_CLK_HZ).unwrap();
        // SAFETY: as above.
        unsafe { console::init(map::UART0_BASE, uart_ref as u32, 115_200) };
        clocks::periph_hz(&slcr, PS_CLK_HZ).unwrap()
    }

    pub fn time_clock(tick_clock_hz: u64) -> u64 {
        tick_clock_hz
    }
}

/// Zynq UltraScale+: the RFSoC 4x2, or QEMU's `xlnx-zcu102` with `--features qemu`.
#[cfg(target_arch = "aarch64")]
mod family {
    use super::ps::{clocks, console, map, mem, timer};
    use hardroxide_mmio::{MmioBackend, PhysMem};

    const PS_REF_HZ: u64 = 33_333_333;

    pub fn console_and_memory() -> u64 {
        // SAFETY: the clock registers are only read; the console UART is used by nothing else.
        let be = unsafe { PhysMem::new() };
        let (uart, uart_ref, counter) = if cfg!(feature = "qemu") {
            // QEMU models neither the low-power clock registers nor a settable counter rate.
            (map::UART0_BASE, 100_000_000, timer::frequency())
        } else {
            let crl = be.map(map::CRL_APB_BASE as u64, map::CRL_APB_SIZE).unwrap();
            let crf = be.map(map::CRF_APB_BASE as u64, map::CRF_APB_SIZE).unwrap();
            (
                map::UART1_BASE,
                clocks::uart_ref_hz(&crl, &crf, PS_REF_HZ, 1).unwrap(),
                clocks::timestamp_hz(&crl, &crf, PS_REF_HZ).unwrap(),
            )
        };
        // SAFETY: early in start-up, at EL3, running from low DDR.
        unsafe { mem::enable(4 << 30) };
        // SAFETY: as above.
        unsafe { console::init(uart, uart_ref as u32, 115_200) };
        counter
    }

    pub fn time_clock(tick_clock_hz: u64) -> u64 {
        tick_clock_hz
    }
}
