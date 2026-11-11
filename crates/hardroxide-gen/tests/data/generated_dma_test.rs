use hardroxide_dma::MockBuffer;
use hardroxide_ip::dma::Direction;
use hardroxide_mmio::MockBackend;
use pynq_z2_dma_test::{Design, pac};
use std::time::Duration;

#[test]
fn dma_transfer_through_generated_design() {
    let be = MockBackend::new();
    let base = pac::axi_dma_0::BASE_ADDRESS;
    be.poke(base + 0x04, 1); // MM2S halted
    be.set_write1_clear(base + 0x04);
    let d = Design::map_only(be.clone()).unwrap();
    assert_eq!(d.axi_dma_0.max_transfer_len(), (1 << 23) - 1, "length width comes from the HWH");
    let t = d.axi_dma_0.start(Direction::MemToStream, MockBuffer::new(256, 0x1F00_0000), 256).unwrap();
    assert_eq!((be.peek(base + 0x18), be.peek(base + 0x28)), (0x1F00_0000, 256));
    be.poke(base + 0x04, 0b10 | (1 << 12)); // idle + IOC
    t.wait(Duration::from_millis(50)).unwrap();
    assert_eq!(pac::axi_dma_0::mm2s_dmacr::OFFSET, 0);
    assert_eq!(pac::axi_dma_0::s2mm_length::OFFSET, 0x58);
    assert_eq!(pac::axi_dma_0::INTERRUPTS, &[("mm2s_introut", Some(0)), ("s2mm_introut", Some(1))]);
}
