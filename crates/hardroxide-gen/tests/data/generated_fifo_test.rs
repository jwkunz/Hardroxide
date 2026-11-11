use hardroxide_mmio::MockBackend;
use pynq_z2_fifo_test::{Design, pac};

#[test]
fn fifo_driver_is_wired_up() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    let base = pac::axi_fifo_mm_s_0::BASE_ADDRESS;
    be.poke(base + 0x0C, 16); // TDFV: 16 words of room
    d.axi_fifo_mm_s_0.send(&[1, 2, 3, 4, 5]).unwrap();
    assert_eq!(be.peek(base + 0x14), 5); // TLR: byte length
    assert_eq!(be.peek(base + 0x10), 5); // the last TDFD write was the zero-padded tail word
    // Not enough room is refused.
    be.poke(base + 0x0C, 1);
    assert!(d.axi_fifo_mm_s_0.send(&[0; 8]).is_err());
}
