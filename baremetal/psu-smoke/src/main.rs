//! Smoke test for hardroxide-psu on a Zynq UltraScale+: console, clocks, time source, MMU and
//! caches, a DDR test in both address ranges, GIC and generic-timer interrupts with floating
//! point live across them, and the exception report. Runs under QEMU (`--features qemu`, see
//! `scripts/test-qemu.sh`) and on the RFSoC 4x2 over JTAG.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use core::time::Duration;
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{MmioBackend, PhysMem};
use hardroxide_psu::gic::Gic;
use hardroxide_psu::{clocks, console, cpu, irq, map, mem, println, time, timer};

#[cfg(feature = "pl")]
mod pl;

hardroxide_psu::entry!(main);

const QEMU: bool = cfg!(feature = "qemu");
/// The PS reference clock of the RFSoC 4x2.
const PS_REF_HZ: u64 = 33_333_333;
/// The DDR to map: 4 GiB on the RFSoC 4x2 (and given to QEMU).
const DDR_BYTES: usize = 4 << 30;
const PERIOD: Duration = Duration::from_millis(10);

static TICKS: AtomicU32 = AtomicU32::new(0);
static HANDLER_FLOAT: AtomicU64 = AtomicU64::new(0);

fn tick() {
    timer::tick();
    // Floating-point work in the handler, in many registers: the entry must preserve the
    // program's.
    let n = f64::from(TICKS.fetch_add(1, Ordering::SeqCst) + 1);
    let mut r = [n; 24];
    for (i, v) in r.iter_mut().enumerate() {
        *v = core::hint::black_box(*v * (i as f64 + 1.5));
    }
    HANDLER_FLOAT.store(r.iter().sum::<f64>().to_bits(), Ordering::SeqCst);
    overwrite_vector_registers();
}

/// Overwrite every vector register, whole. The compiler keeps only the lower halves of
/// v8..v15 across this (all the calling convention asks), so the upper halves are the
/// interrupt entry's to preserve.
fn overwrite_vector_registers() {
    // SAFETY: writes registers only, all declared.
    unsafe {
        core::arch::asm!(
            ".irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31",
            "movi v\\n\\().16b, #0xA5",
            ".endr",
            out("v0") _, out("v1") _, out("v2") _, out("v3") _, out("v4") _, out("v5") _, out("v6") _, out("v7") _,
            out("v8") _, out("v9") _, out("v10") _, out("v11") _, out("v12") _, out("v13") _, out("v14") _, out("v15") _,
            out("v16") _, out("v17") _, out("v18") _, out("v19") _, out("v20") _, out("v21") _, out("v22") _, out("v23") _,
            out("v24") _, out("v25") _, out("v26") _, out("v27") _, out("v28") _, out("v29") _, out("v30") _, out("v31") _,
            options(nomem, nostack)
        );
    }
}

/// Fill all 32 vector registers, wait with them loaded until another tick has been handled,
/// and compare. Interrupts must be enabled and the timer running.
fn vector_registers_survive_an_interrupt() -> bool {
    #[repr(C, align(16))]
    struct Registers([u64; 64]);
    let mut before = Registers([0; 64]);
    for (i, v) in before.0.iter_mut().enumerate() {
        *v = expected(i as u64, 11);
    }
    let mut after = Registers([0; 64]);
    let target = TICKS.load(Ordering::SeqCst) + 1;
    // SAFETY: loads from and stores to the two 512-byte arrays and reads the tick count;
    // every register written is declared.
    unsafe {
        core::arch::asm!(
            "ld1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [{from}], #64",
            "ld1 {{v4.16b, v5.16b, v6.16b, v7.16b}}, [{from}], #64",
            "ld1 {{v8.16b, v9.16b, v10.16b, v11.16b}}, [{from}], #64",
            "ld1 {{v12.16b, v13.16b, v14.16b, v15.16b}}, [{from}], #64",
            "ld1 {{v16.16b, v17.16b, v18.16b, v19.16b}}, [{from}], #64",
            "ld1 {{v20.16b, v21.16b, v22.16b, v23.16b}}, [{from}], #64",
            "ld1 {{v24.16b, v25.16b, v26.16b, v27.16b}}, [{from}], #64",
            "ld1 {{v28.16b, v29.16b, v30.16b, v31.16b}}, [{from}], #64",
            "2:",
            "ldr {t:w}, [{ticks}]",
            "cmp {t:w}, {target:w}",
            "b.lo 2b",
            "st1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [{to}], #64",
            "st1 {{v4.16b, v5.16b, v6.16b, v7.16b}}, [{to}], #64",
            "st1 {{v8.16b, v9.16b, v10.16b, v11.16b}}, [{to}], #64",
            "st1 {{v12.16b, v13.16b, v14.16b, v15.16b}}, [{to}], #64",
            "st1 {{v16.16b, v17.16b, v18.16b, v19.16b}}, [{to}], #64",
            "st1 {{v20.16b, v21.16b, v22.16b, v23.16b}}, [{to}], #64",
            "st1 {{v24.16b, v25.16b, v26.16b, v27.16b}}, [{to}], #64",
            "st1 {{v28.16b, v29.16b, v30.16b, v31.16b}}, [{to}], #64",
            from = inout(reg) before.0.as_ptr() => _,
            to = inout(reg) after.0.as_mut_ptr() => _,
            ticks = in(reg) TICKS.as_ptr(),
            target = in(reg) target,
            t = out(reg) _,
            out("v0") _, out("v1") _, out("v2") _, out("v3") _, out("v4") _, out("v5") _, out("v6") _, out("v7") _,
            out("v8") _, out("v9") _, out("v10") _, out("v11") _, out("v12") _, out("v13") _, out("v14") _, out("v15") _,
            out("v16") _, out("v17") _, out("v18") _, out("v19") _, out("v20") _, out("v21") _, out("v22") _, out("v23") _,
            out("v24") _, out("v25") _, out("v26") _, out("v27") _, out("v28") _, out("v29") _, out("v30") _, out("v31") _,
            options(nostack)
        );
    }
    before.0 == after.0
}

/// Floating-point sums that keep many registers busy; the same input must give the same bits.
#[inline(never)]
fn float_work() -> u64 {
    let mut r = [1.0f64; 24];
    for k in 0..2000u32 {
        for (i, v) in r.iter_mut().enumerate() {
            *v = core::hint::black_box(*v * 0.999_9 + f64::from(k % 7) * (i as f64 + 1.0) * 1e-3);
        }
    }
    r.iter().sum::<f64>().to_bits()
}

/// A word that depends on its address, so that two addresses reaching one cell show up.
fn expected(address: u64, pass: u64) -> u64 {
    (address ^ pass.wrapping_mul(0x9E37_79B9_7F4A_7C15)).wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Write, push out to the DDR, read back from the DDR: `len` bytes at `base`. Returns the
/// number of wrong words.
fn test_region(base: u64, len: usize, pass: u64) -> usize {
    let words = len / 8;
    let at = base as *mut u64;
    for i in 0..words {
        // SAFETY: the caller names DDR that nothing else uses.
        unsafe {
            at.add(i)
                .write_volatile(expected(base + 8 * i as u64, pass))
        };
    }
    mem::flush_range(base as usize, len);
    (0..words)
        // SAFETY: as above.
        .filter(|&i| unsafe { at.add(i).read_volatile() } != expected(base + 8 * i as u64, pass))
        .count()
}

/// One word at every power of two across both DDR ranges, all written before any is read:
/// catches address lines that alias.
fn test_address_lines() -> usize {
    let spots = (27..31)
        .map(|bit| 1u64 << bit) // low DDR above the program
        .chain((12..31).map(|bit| (32u64 << 30) + (1 << bit)))
        .chain([32u64 << 30]);
    for a in spots.clone() {
        // SAFETY: DDR beyond the program (which ends below 128 MiB).
        unsafe { (a as *mut u64).write_volatile(expected(a, 77)) };
        mem::flush_range(a as usize, 8);
    }
    spots
        // SAFETY: as above.
        .filter(|&a| unsafe { (a as *const u64).read_volatile() } != expected(a, 77))
        .count()
}

fn main() -> ! {
    // SAFETY: the clock registers are only read; the console UART is used by nothing else.
    let be = unsafe { PhysMem::new() };
    let (uart, uart_ref, counter_hz, cpu_hz) = if QEMU {
        // QEMU models neither the low-power clock registers nor a settable counter rate.
        (map::UART0_BASE, 100_000_000, timer::frequency(), 0)
    } else {
        let crl = be.map(map::CRL_APB_BASE as u64, map::CRL_APB_SIZE).unwrap();
        let crf = be.map(map::CRF_APB_BASE as u64, map::CRF_APB_SIZE).unwrap();
        (
            map::UART1_BASE,
            clocks::uart_ref_hz(&crl, &crf, PS_REF_HZ, 1).unwrap(),
            clocks::timestamp_hz(&crl, &crf, PS_REF_HZ).unwrap(),
            clocks::cpu_hz(&crl, &crf, PS_REF_HZ).unwrap(),
        )
    };
    // SAFETY: as above.
    let baud = unsafe { console::init(uart, uart_ref as u32, 115_200) };
    println!("\n== hardroxide-psu smoke test ==");
    println!("uart reference {uart_ref} Hz, baud {baud:?}");
    println!(
        "cpu {cpu_hz} Hz, counter {counter_hz} Hz, core {}, EL{}",
        cpu::cpu_id(),
        cpu::current_el()
    );
    println!("SCTLR {:#010x}", cpu::sctlr());
    let mut ok = true;

    // Time source.
    // SAFETY: at EL3, core 0, before anything reads the time.
    unsafe { time::init(counter_hz) };
    let t0 = Instant::now();
    sleep(Duration::from_millis(100));
    println!("sleep(100 ms) took {} us", t0.elapsed().as_micros());

    // Caches: the same memory-bound loop with the caches off, then on.
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
    // SAFETY: early in start-up, at EL3, running from low DDR.
    unsafe { mem::enable(DDR_BYTES) };
    let (a1, on_us) = sum(20);
    let dma = hardroxide_dma::static_dma_buffer!(4096).unwrap();
    let dma_addr = hardroxide_dma::DmaBuffer::phys_addr(&dma);
    let aligned = dma_addr % mem::SECTION as u64 == 0;
    println!(
        "caches: SCTLR {:#010x}; 20 passes over 64 KiB: {off_us} us off, {on_us} us on; checksums {}; dma buffer at {dma_addr:#010x}, 2 MiB aligned: {aligned}",
        cpu::sctlr(),
        if a0 == a1 { "match" } else { "DIFFER" },
    );
    ok &= a0 == a1 && aligned && mem::is_enabled();

    // DDR: regions in both ranges, and the address lines.
    let size = if QEMU { 1 << 20 } else { 64 << 20 };
    for (name, base) in [
        ("low DDR", 0x1000_0000u64),
        ("top of low DDR", 0x8000_0000 - size as u64),
        ("high DDR", 32 << 30),
        ("top of high DDR", (34 << 30) - size as u64),
    ] {
        let t = Instant::now();
        let bad = test_region(base, size, 1) + test_region(base, size, 2);
        println!(
            "memory test, {name}: {} MiB twice at {base:#012x}: {bad} errors ({} ms)",
            size >> 20,
            t.elapsed().as_millis()
        );
        ok &= bad == 0;
    }
    let bad = test_address_lines();
    println!("memory test, address lines: {bad} errors");
    ok &= bad == 0;

    // A block made non-cacheable at run time still works.
    const UNCACHED: usize = 0x2000_0000;
    // SAFETY: DDR nothing else uses; the MMU is on.
    unsafe { mem::set_non_cacheable(UNCACHED, 4096) };
    let bad = test_region(UNCACHED as u64, mem::SECTION, 3);
    println!("non-cacheable block: {bad} errors");
    ok &= bad == 0;

    // Interrupts: a periodic secure-timer tick, with floating point running.
    let want_float = float_work();
    let d = be
        .map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE)
        .unwrap();
    let c = be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE).unwrap();
    let gic = Gic::new(d, c);
    gic.init().unwrap();
    let id = map::ppi::SECURE_PHYSICAL_TIMER;
    assert!(irq::register(id, tick));
    gic.set_priority(id, 0x40).unwrap();
    gic.enable(id).unwrap();
    let t1 = Instant::now();
    timer::start_periodic(timer::duration_to_ticks(PERIOD, counter_hz));
    // SAFETY: the handler and the GIC are set up.
    unsafe { cpu::enable_irq() };
    let mut rounds = 0u32;
    let mut wrong = 0u32;
    while TICKS.load(Ordering::SeqCst) < 20 {
        wrong += u32::from(float_work() != want_float);
        rounds += 1;
    }
    let (span, ticks) = (t1.elapsed(), TICKS.load(Ordering::SeqCst));
    let vectors = (0..5).all(|_| vector_registers_survive_an_interrupt());
    timer::stop();
    cpu::disable_irq();
    println!(
        "timer interrupts: {ticks} in {} us (period target 10000 us)",
        span.as_micros()
    );
    println!("floating point across interrupts: {wrong} wrong results in {rounds} rounds");
    println!("all 32 vector registers whole across 5 interrupts: {vectors}");
    println!("unhandled interrupts: {}", irq::unhandled_count());
    ok &= wrong == 0 && rounds > 0 && vectors && irq::unhandled_count() == 0;

    // `wfi` wakes on the next tick.
    timer::start_periodic(timer::duration_to_ticks(PERIOD, counter_hz));
    // SAFETY: as above.
    unsafe { cpu::enable_irq() };
    let before = TICKS.load(Ordering::SeqCst);
    while TICKS.load(Ordering::SeqCst) < before + 3 {
        cpu::wfi();
    }
    timer::stop();
    cpu::disable_irq();
    println!("wfi woke on the timer three times");

    #[cfg(feature = "pl")]
    {
        ok &= pl::test(&gic);
    }

    println!("{}", if ok { "ALL OK" } else { "FAILED" });

    println!("-- executing an undefined instruction on purpose");
    // SAFETY: deliberately raises an exception to test the report.
    unsafe { core::arch::asm!(".inst 0x00000000") };
    println!("not reached");
    loop {
        cpu::wfe();
    }
}
