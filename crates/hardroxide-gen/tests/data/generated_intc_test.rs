use hardroxide_irq::IrqError;
use hardroxide_mmio::MockBackend;
use pynq_z2_intc_test::{Design, pac};

#[test]
fn sources_are_routed_through_the_controller() {
    // Each device interrupt is an input of axi_intc_0, with no UIO of its own.
    assert_eq!(pac::axi_timer_0::IRQ_INTERRUPT.via_intc, Some(("axi_intc_0", 0)));
    assert_eq!(pac::axi_iic_0::IRQ_IIC2INTC_IRPT.via_intc, Some(("axi_intc_0", 1)));
    assert_eq!(pac::axi_gpio_0::IRQ_IP2INTC_IRPT.via_intc, Some(("axi_intc_0", 2)));
    let e = pac::axi_timer_0::IRQ_INTERRUPT.open().err().unwrap();
    assert!(matches!(e, IrqError::ViaController { bit: 0, .. }));
    // The controller's own output goes to the PS: IRQ_F2P[0] -> SPI 29.
    let own = pac::axi_intc_0::IRQ_IRQ;
    assert_eq!((own.via_intc, own.f2p_bit, own.gic_spi), (None, Some(0), Some(29)));
}

#[test]
fn controller_driver_is_wired_up() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    let base = pac::axi_intc_0::BASE_ADDRESS;
    d.axi_intc_0.init().unwrap();
    assert_eq!(be.peek(base + 0x1C), 0b11); // MER: ME | HIE
    d.axi_intc_0.enable(0b101).unwrap();
    assert_eq!(be.peek(base + 0x10), 0b101); // SIE
    be.poke(base + 0x04, 0b100); // IPR: GPIO input pending
    assert_eq!(d.axi_intc_0.pending().unwrap(), 0b100);
}
