//! Run the HLS `vadd` kernel and wait for its done interrupt.
#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, pac, psu};
use hardroxide_irq::IrqWaiter;
use psu::println;
use psu::wait::IrqWait;

psu::entry!(main);

fn main() -> ! {
    let d = Design::init().expect("design");
    let v = &d.vadd_0;
    let irq = IrqWait::for_line(&pac::vadd_0::IRQ_INTERRUPT).expect("interrupt");
    d.enable_interrupts();
    v.enable_done_interrupt().unwrap();
    for i in 0..100u32 {
        v.set_a(i.wrapping_mul(1_000_003)).unwrap();
        v.set_b(41).unwrap();
        irq.enable().unwrap();
        v.start().unwrap();
        let event = irq.wait_event(Some(Duration::from_millis(100))).unwrap();
        assert!(event.is_some(), "no interrupt in round {i}");
        v.ack_interrupts().unwrap();
        assert_eq!(
            v.out_r().unwrap(),
            i.wrapping_mul(1_000_003).wrapping_add(41),
            "round {i}"
        );
    }
    println!("100 additions, each ending in an interrupt");
    println!("HLS VADD OK");
    design_1::halt()
}
