//! Smoke test for hardroxide-ps7 on a Zynq-7000: console, clocks, time source, GIC and
//! private-timer interrupts, and the exception report. Runs under QEMU
//! (`--features qemu`, see `scripts/test-qemu.sh`) and on the board over JTAG.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem};
use hardroxide_ps7::gic::Gic;
use hardroxide_ps7::timer::{PrivateTimer, duration_to_ticks};
use hardroxide_ps7::{clocks, console, cp15, irq, map, println, time};

hardroxide_ps7::entry!(main);

/// The PS reference clock: 50 MHz on the PYNQ-Z2, 33.33 MHz in QEMU's Zynq model.
const PS_CLK_HZ: u64 = if cfg!(feature = "qemu") {
    33_333_333
} else {
    50_000_000
};

static TICKS: AtomicU32 = AtomicU32::new(0);

fn tick() {
    // SAFETY: the private timer window is used only here and in `main`.
    let be = unsafe { PhysMem::new() };
    let t = PrivateTimer::new(
        be.map(map::PRIVATE_TIMER_BASE as u64, map::PRIVATE_TIMER_SIZE)
            .unwrap(),
    );
    let _ = t.clear_interrupt();
    TICKS.fetch_add(1, Ordering::SeqCst);
}

/// What `ps7_init` (or, here, a stand-in) leaves for the program: PLLs running and the
/// UART's clocks and pins set up.
fn platform_init() {
    // SAFETY: this program owns the SLCR for the duration of this setup.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let w = |o: usize, v: u32| slcr.write32(o, v).unwrap();
    let r = |o: usize| slcr.read32(o).unwrap();
    w(0x008, 0xDF0D); // unlock
    if cfg!(feature = "qemu") {
        // The QEMU model gates the UART until the IO PLL has been programmed, as ps7_init does.
        let m = |a: usize, mask: u32, v: u32| w(a, (r(a) & !mask) | (v & mask));
        m(0x108, 0x0007_F000, 0x0002_0000);
        m(0x108, 0x10, 0x10);
        m(0x108, 0x1, 0x1);
        m(0x108, 0x1, 0x0);
        m(0x108, 0x10, 0x0);
    }
    // The UART's bus clock (cleared by a ps7_init that does not use UART0).
    w(0x12C, r(0x12C) | (1 << 20));
    // UART clock: IO PLL divided to 100 MHz.
    let io_fdiv = (r(0x108) >> 12) & 0x7F;
    w(0x154, ((io_fdiv / 2).max(1) << 8) | 0x3);
    w(0x228, 0); // release the UART resets
    // UART0 on MIO 14 (RX) and 15 (TX): 3.3 V, pull-up, function 7.
    w(0x738, 0x16E1);
    w(0x73C, 0x16E0);
    w(0x004, 0x767B); // lock
}

fn main() -> ! {
    platform_init();
    // SAFETY: SLCR is only read here; the console UART is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let slcr = be.map(map::SLCR_BASE as u64, map::SLCR_SIZE).unwrap();
    let uart_ref = clocks::uart_ref_hz(&slcr, PS_CLK_HZ).unwrap();
    let baud = unsafe { console::init(map::UART0_BASE, uart_ref as u32, 115_200) };

    println!("\n== hardroxide-ps7 smoke test ==");
    println!("uart reference {} Hz, baud {:?}", uart_ref, baud);
    let cpu = clocks::cpu_hz(&slcr, PS_CLK_HZ).unwrap();
    let periph = clocks::periph_hz(&slcr, PS_CLK_HZ).unwrap();
    println!(
        "cpu {cpu} Hz, private peripherals {periph} Hz, core {}",
        cp15::cpu_id()
    );
    println!("SCTLR {:#010x}, CPSR {:#010x}", cp15::sctlr(), cp15::cpsr());

    // Time source.
    // SAFETY: nothing else touches the global timer.
    unsafe { time::init(periph) };
    let t0 = Instant::now();
    sleep(Duration::from_millis(100));
    let took = t0.elapsed();
    println!("sleep(100 ms) took {} us", took.as_micros());

    // Interrupts: a 10 ms periodic private-timer tick, counted by the handler.
    let d = be
        .map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE)
        .unwrap();
    let c = be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE).unwrap();
    let gic = Gic::new(d, c);
    gic.init().unwrap();
    assert!(irq::register(map::ppi::PRIVATE_TIMER, tick));
    gic.set_priority(map::ppi::PRIVATE_TIMER, 0x40).unwrap();
    gic.enable(map::ppi::PRIVATE_TIMER).unwrap();
    let pt = PrivateTimer::new(
        be.map(map::PRIVATE_TIMER_BASE as u64, map::PRIVATE_TIMER_SIZE)
            .unwrap(),
    );
    let ticks_10ms = duration_to_ticks(Duration::from_millis(10), periph) as u32;
    let t1 = Instant::now();
    pt.start_periodic(ticks_10ms).unwrap();
    // SAFETY: the handler and the GIC are set up.
    unsafe { cp15::enable_irq() };
    while TICKS.load(Ordering::SeqCst) < 20 {
        cp15::wfi();
    }
    let span = t1.elapsed();
    pt.stop().unwrap();
    cp15::disable_irq();
    println!(
        "timer interrupts: {} in {} us (period target 10000 us)",
        TICKS.load(Ordering::SeqCst),
        span.as_micros()
    );
    println!("unhandled interrupts: {}", irq::unhandled_count());
    println!("ALL OK");

    println!("-- executing an undefined instruction on purpose");
    // SAFETY: deliberately raises an undefined-instruction exception to test the report.
    unsafe { core::arch::asm!(".word 0xe7f000f0") };
    println!("not reached");
    loop {
        cp15::wfe();
    }
}
