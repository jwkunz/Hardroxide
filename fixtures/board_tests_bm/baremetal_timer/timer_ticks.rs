//! Bare metal: a PL timer interrupt every 10 ms, handled through the GIC; reports how regular
//! the events are. Bare metal tutorial 2.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::time::Instant;
use ps7::println;
use ps7::wait::IrqWait;

ps7::entry!(main);

fn main() -> ! {
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("Design::init failed: {e}");
            halt()
        }
    };
    let timer = &d.axi_timer_0;
    let irq = IrqWait::for_line(&design_1::pac::axi_timer_0::IRQ_INTERRUPT).expect("interrupt reaches the PS");
    d.enable_interrupts();

    println!("\n== timer_ticks: 200 interrupts at 10 ms ==");
    timer.clear().unwrap();
    irq.enable().unwrap();
    // Auto-reload: the PL timer restarts by itself, so the period does not drift with the CPU.
    timer.start(timer.ticks_for_ns(10_000_000) as u32, true, true).unwrap();

    let t0 = Instant::now();
    let mut last = 0u64;
    let (mut min, mut max) = (u64::MAX, 0u64);
    for n in 1..=200 {
        irq.wait_event(Some(Duration::from_secs(1))).unwrap();
        let now = t0.elapsed().as_micros() as u64;
        timer.clear().unwrap(); // acknowledge the PL interrupt flag...
        irq.enable().unwrap(); // ...and unmask the line again
        if n > 1 {
            let gap = now - last;
            min = min.min(gap);
            max = max.max(gap);
        }
        last = now;
    }
    timer.stop().unwrap();
    println!("200 events in {} us; gap between events: min {min} us, max {max} us", t0.elapsed().as_micros());
    halt()
}
