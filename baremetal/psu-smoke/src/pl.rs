//! The programmable logic, with `fixtures/rfsoc4x2/rfsoc4x2_loopback.xsa` loaded: registers
//! through HPM0_LPD, and its interrupts through the GIC both ways the drivers use them, as a
//! handler and as a counted event a program waits for.

use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::{Mmio, MmioBackend, PhysMem, PhysRegion};
use hardroxide_psu::gic::Gic;
use hardroxide_psu::wait::IrqWait;
use hardroxide_psu::{cpu, irq, map, println};

// The design's address map.
const TIMER: u64 = 0x8000_0000;
const GPIO_IN: u64 = 0x8006_0000;
const GPIO_OUT: u64 = 0x8007_0000;
const BRAM: u64 = 0x8200_0000;
// Its interrupts: bits of pl_ps_irq0.
const TIMER_IRQ: u32 = 0;
const GPIO_IRQ: u32 = 6;

const GPIO_ISR: usize = 0x120;
const GPIO_IER: usize = 0x128;
const GPIO_GIER: usize = 0x11C;

static GPIO_INTERRUPTS: AtomicU32 = AtomicU32::new(0);

fn region(base: u64, len: usize) -> PhysRegion {
    // SAFETY: PL register windows of the loaded design, mapped as device memory; the handler
    // and the program use the GPIO's status register only one after the other.
    unsafe { PhysMem::new() }.map(base, len).unwrap()
}

fn gpio_handler() {
    // Acknowledge: the status bits toggle when written with one.
    let g = region(GPIO_IN, 0x1000);
    let status = g.read32(GPIO_ISR).unwrap();
    g.write32(GPIO_ISR, status).unwrap();
    GPIO_INTERRUPTS.fetch_add(1, Ordering::SeqCst);
}

pub fn test(gic: &Gic<PhysRegion>) -> bool {
    let (input, output) = (region(GPIO_IN, 0x1000), region(GPIO_OUT, 0x1000));
    let mut ok = true;

    let ram = region(BRAM, 0x1_0000);
    for i in 0..0x4000 {
        ram.write32(4 * i, (i as u32).wrapping_mul(0x9E37_79B9))
            .unwrap();
    }
    let bad = (0..0x4000)
        .filter(|&i| ram.read32(4 * i).unwrap() != (i as u32).wrapping_mul(0x9E37_79B9))
        .count();
    println!("PL block RAM, 64 KiB: {bad} errors");
    ok &= bad == 0;

    output.write32(0, 0xA5).unwrap();
    let read = input.read32(0).unwrap();
    println!("PL GPIO loop: wrote 0xa5, read {read:#x}");
    ok &= read == 0xA5;

    // A handler: ten changes of the input, ten interrupts.
    let id = map::pl_irq(GPIO_IRQ);
    input
        .write32(GPIO_ISR, input.read32(GPIO_ISR).unwrap())
        .unwrap();
    input.write32(GPIO_IER, 1).unwrap();
    input.write32(GPIO_GIER, 0x8000_0000).unwrap();
    assert!(irq::register(id, gpio_handler));
    gic.set_priority(id, 0x80).unwrap();
    gic.enable(id).unwrap();
    // SAFETY: the handler and the GIC are set up.
    unsafe { cpu::enable_irq() };
    for n in 1..=10u32 {
        output.write32(0, n).unwrap();
        let mut spins = 0u32;
        while GPIO_INTERRUPTS.load(Ordering::SeqCst) < n && spins < 10_000_000 {
            spins += 1;
        }
    }
    let got = GPIO_INTERRUPTS.load(Ordering::SeqCst);
    println!("PL GPIO interrupt (ID {id}), handler: {got} of 10");
    ok &= got == 10;
    gic.disable(id).unwrap();

    // An event the program waits for: the timer, three times, with a wait that times out
    // in between.
    let timer = region(TIMER, 0x1000);
    let wait = IrqWait::new(map::pl_irq(TIMER_IRQ)).unwrap();
    gic.set_priority(wait.id(), 0x80).unwrap();
    let quiet = {
        wait.enable().unwrap();
        wait.wait_event(Some(Duration::from_millis(20))).unwrap()
    };
    let mut events = 0;
    for _ in 0..3 {
        timer.write32(4, 0xFFF0_0000).unwrap(); // load value: 10.5 ms before overflow at 100 MHz
        timer.write32(0, 0x20).unwrap(); // load it
        timer.write32(0, 0xC0).unwrap(); // run, interrupt on overflow
        wait.enable().unwrap();
        let n = wait.wait_event(Some(Duration::from_secs(1))).unwrap();
        timer.write32(0, 0x100).unwrap(); // clear and stop
        events += u32::from(n.is_some());
    }
    cpu::disable_irq();
    println!(
        "PL timer interrupt (ID {}), waited for: {events} of 3; nothing before the timer ran: {}",
        wait.id(),
        quiet.is_none()
    );
    ok &= events == 3 && quiet.is_none();
    println!("unhandled interrupts: {}", irq::unhandled_count());
    ok && irq::unhandled_count() == 0
}
