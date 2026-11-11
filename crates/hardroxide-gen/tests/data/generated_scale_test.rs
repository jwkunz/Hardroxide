use hardroxide_dma::MockBuffer;
use hardroxide_mmio::MockBackend;
use pynq_z2_scale_test::{Design, pac};

#[test]
fn pointer_arguments_become_64_bit_address_setters() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    let base = pac::scale_0::BASE_ADDRESS;
    // in_r at 0x10/0x14, out_r at 0x1c/0x20 (per the HWH); n and k are scalars.
    d.scale_0.set_in_r_addr(0x1_2345_6780).unwrap();
    assert_eq!(be.peek(base + 0x10), 0x2345_6780);
    assert_eq!(be.peek(base + 0x14), 1);
    assert_eq!(d.scale_0.in_r_addr().unwrap(), 0x1_2345_6780);
    let buf = MockBuffer::new(64, 0x1f00_0000);
    d.scale_0.set_out_r_buffer(&buf).unwrap();
    assert_eq!(d.scale_0.out_r_addr().unwrap(), 0x1f00_0000);
    d.scale_0.set_n(16).unwrap();
    d.scale_0.set_k(3).unwrap();
    assert_eq!((be.peek(base + 0x28), be.peek(base + 0x30)), (16, 3));
    // The raw halves are not exposed as separate scalar setters.
    d.scale_0.start().unwrap();
    assert_eq!(be.peek(base) & 1, 1);
}
