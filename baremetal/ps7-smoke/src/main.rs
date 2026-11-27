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
    overwrite_double_registers();
    TICKS.fetch_add(1, Ordering::SeqCst);
}

/// Overwrite every double register, as a handler may: the interrupt entry has to preserve d0-d7 and d16-d31, and the
/// compiler keeps d8-d15 across this.
fn overwrite_double_registers() {
    static JUNK: [u64; 16] = [0xA5A5_A5A5_A5A5_A5A5; 16];
    // SAFETY: loads registers, all declared, from a static of the size loaded.
    unsafe {
        core::arch::asm!(
            "vldmia {junk}, {{d0-d15}}",
            "vldmia {junk}, {{d16-d31}}",
            junk = in(reg) JUNK.as_ptr(),
            out("d0") _, out("d1") _, out("d2") _, out("d3") _, out("d4") _, out("d5") _, out("d6") _, out("d7") _,
            out("d8") _, out("d9") _, out("d10") _, out("d11") _, out("d12") _, out("d13") _, out("d14") _, out("d15") _,
            out("d16") _, out("d17") _, out("d18") _, out("d19") _, out("d20") _, out("d21") _, out("d22") _, out("d23") _,
            out("d24") _, out("d25") _, out("d26") _, out("d27") _, out("d28") _, out("d29") _, out("d30") _, out("d31") _,
            options(nostack)
        );
    }
}

/// Fill all 32 double registers, wait with them loaded until another tick has been handled, and compare. Interrupts must
/// be enabled and the timer running.
fn double_registers_survive_an_interrupt() -> bool {
    let before: [u64; 32] =
        core::array::from_fn(|i| (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut after = [0u64; 32];
    let target = TICKS.load(Ordering::SeqCst) + 1;
    // SAFETY: loads from and stores to the two 256-byte arrays and reads the tick count; every register written is
    // declared.
    unsafe {
        core::arch::asm!(
            "vldmia {from}!, {{d0-d15}}",
            "vldmia {from}, {{d16-d31}}",
            "2:",
            "ldr {seen}, [{ticks}]",
            "cmp {seen}, {target}",
            "blo 2b",
            "vstmia {to}!, {{d0-d15}}",
            "vstmia {to}, {{d16-d31}}",
            from = inout(reg) before.as_ptr() => _,
            to = inout(reg) after.as_mut_ptr() => _,
            ticks = in(reg) TICKS.as_ptr(),
            target = in(reg) target,
            seen = out(reg) _,
            out("d0") _, out("d1") _, out("d2") _, out("d3") _, out("d4") _, out("d5") _, out("d6") _, out("d7") _,
            out("d8") _, out("d9") _, out("d10") _, out("d11") _, out("d12") _, out("d13") _, out("d14") _, out("d15") _,
            out("d16") _, out("d17") _, out("d18") _, out("d19") _, out("d20") _, out("d21") _, out("d22") _, out("d23") _,
            out("d24") _, out("d25") _, out("d26") _, out("d27") _, out("d28") _, out("d29") _, out("d30") _, out("d31") _,
            options(nostack)
        );
    }
    before == after
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

    // Caches: the same memory-bound loop with the caches off, then on. A buffer in `.dma`
    // stays non-cacheable and 1 MiB aligned.
    static mut WORK: [u32; 16384] = [1; 16384];
    let sum = |rounds: u32| {
        let t = Instant::now();
        let mut acc = 0u32;
        for _ in 0..rounds {
            // SAFETY: single-threaded here; WORK is only used by this closure.
            let w = unsafe { &*core::ptr::addr_of!(WORK) };
            for v in w.iter() {
                acc = acc.wrapping_add(core::hint::black_box(*v));
            }
        }
        (acc, t.elapsed().as_micros())
    };
    let (a0, off_us) = sum(20);
    // SAFETY: early in start-up, running from DDR at its physical address.
    unsafe { hardroxide_ps7::mem::enable(512 << 20) };
    let (a1, on_us) = sum(20);
    let dma = hardroxide_dma::static_dma_buffer!(4096).unwrap();
    let dma_addr = hardroxide_dma::DmaBuffer::phys_addr(&dma);
    println!(
        "caches: SCTLR {:#010x}; 20 passes over 64 KiB: {off_us} us off, {on_us} us on; checksums {}; dma buffer at {dma_addr:#010x}, 1 MiB aligned: {}",
        cp15::sctlr(),
        if a0 == a1 { "match" } else { "DIFFER" },
        dma_addr % (1 << 20) == 0
    );

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
    let ticks = TICKS.load(Ordering::SeqCst);
    let registers = (0..5)
        .filter(|_| double_registers_survive_an_interrupt())
        .count()
        == 5;
    pt.stop().unwrap();
    cp15::disable_irq();
    println!(
        "timer interrupts: {} in {} us (period target 10000 us)",
        ticks,
        span.as_micros()
    );
    println!("all 32 double registers whole across 5 interrupts: {registers}");
    println!("unhandled interrupts: {}", irq::unhandled_count());
    println!("{}", if registers { "ALL OK" } else { "FAILED" });

    println!("-- executing an undefined instruction on purpose");
    // SAFETY: deliberately raises an undefined-instruction exception to test the report.
    unsafe { core::arch::asm!(".word 0xe7f000f0") };
    println!("not reached");
    loop {
        cp15::wfe();
    }
}
