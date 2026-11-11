use hardroxide_mmio::MockBackend;
use pynq_z2_hls_test::{Design, pac};
use std::time::Duration;

#[test]
fn vadd_registers_and_control() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    let base = pac::vadd_0::BASE_ADDRESS;
    d.vadd_0.set_a(40).unwrap();
    d.vadd_0.set_b(2).unwrap();
    assert_eq!((be.peek(base + 0x10), be.peek(base + 0x18)), (40, 2));
    d.vadd_0.start().unwrap();
    assert_eq!(be.peek(base) & 1, 1);
    // Fake the hardware finishing.
    be.poke(base, 0b110);
    be.poke(base + 0x20, 42);
    d.vadd_0.wait_done(Duration::from_millis(10)).unwrap();
    assert_eq!(d.vadd_0.out_r().unwrap(), 42);
    assert_eq!(pac::vadd_0::out_r::OFFSET, 0x20);
    assert_eq!(pac::vadd_0::ctrl::ap_start::MASK, 1);
    // Interrupt routing is recorded in the PAC.
    assert_eq!(pac::vadd_0::INTERRUPTS, &[("interrupt", Some(0))]);
}
