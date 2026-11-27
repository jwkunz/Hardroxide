//! Run the HLS `vadd` kernel and wait for its done interrupt.
use std::time::Duration;
use design_1::{Design, pac};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = Design::load()?;
    let v = &d.vadd_0;
    let irq = pac::vadd_0::IRQ_INTERRUPT.open().expect("interrupt");
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
    Ok(())
}
