use embedded_hal::i2c::I2c;
use embedded_hal::spi::{MODE_0, SpiBus};
use hardroxide_mmio::MockBackend;
use pynq_z2_periph_test::{Design, pac};

#[test]
fn constructors_use_hwh_parameters() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    assert_eq!(d.axi_timer_0.clock_hz(), 100_000_000);
    assert_eq!(pac::axi_timer_0::tcsr0::OFFSET, 0);
    assert_eq!(pac::axi_iic_0::tx_fifo::OFFSET, 0x108);
    assert_eq!(pac::axi_quad_spi_0::spicr::OFFSET, 0x60);
    assert_eq!(
        pac::axi_timer_0::INTERRUPTS.iter().chain(pac::axi_iic_0::INTERRUPTS).chain(pac::axi_quad_spi_0::INTERRUPTS).collect::<Vec<_>>(),
        [&("interrupt", Some(0)), &("iic2intc_irpt", Some(1)), &("ip2intc_irpt", Some(2))]
    );
}

#[test]
fn spi_and_iic_drive_the_mapped_registers() {
    let be = MockBackend::new();
    let mut d = Design::map_only(be.clone()).unwrap();
    // SPI: RX FIFO never empty, data register returns a constant.
    let spi_base = pac::axi_quad_spi_0::BASE_ADDRESS;
    be.poke(spi_base + 0x64, 0); // SPISR: RX not empty
    be.poke(spi_base + 0x6C, 0x5A);
    d.axi_quad_spi_0.init(MODE_0, false, false).unwrap();
    let mut buf = [0u8; 3];
    d.axi_quad_spi_0.transfer_in_place(&mut buf).unwrap();
    assert_eq!(buf, [0x5A; 3]);
    // IIC: idle bus, empty TX FIFO (SR bit 7), empty RX unless filled.
    let iic_base = pac::axi_iic_0::BASE_ADDRESS;
    be.poke(iic_base + 0x104, 1 << 7);
    d.axi_iic_0.write(0x42, &[0x11]).unwrap();
    assert_eq!(be.peek(iic_base + 0x108), 0x200 | 0x11, "STOP + data in the TX FIFO");
}
