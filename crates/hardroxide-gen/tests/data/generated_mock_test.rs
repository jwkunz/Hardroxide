use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::MockBackend;
use pynq_z2_gpio_test::{Design, pac};

#[test]
fn leds_and_buttons_via_mock() {
    let be = MockBackend::new();
    be.poke(pac::axi_gpio_0::BASE_ADDRESS + 8, 0b10_0101);
    let d = Design::map_only(be.clone()).unwrap();
    d.axi_gpio_0.set_input_mask(Channel::One, 0).unwrap();
    d.axi_gpio_0.write(Channel::One, 0b0110).unwrap();
    assert_eq!(be.peek(0x4120_0000), 0b0110);
    assert_eq!(d.axi_gpio_0.read(Channel::Two).unwrap(), 0b10_0101);
    assert_eq!(pac::axi_gpio_0::gpio_tri::OFFSET, 4);
    assert_eq!(pac::axi_gpio_0::gpio_data::ch1_data::MASK, 0xf);
    assert_eq!(pac::axi_gpio_0::SIZE, 0x10000);
    assert_eq!(pynq_z2_gpio_test::PL_CLOCKS, &[(0, 100_000_000)]);
}

#[test]
fn load_is_a_singleton_and_checks_paths() {
    use pynq_z2_gpio_test::{Error, LoadOptions};
    let root = std::env::temp_dir().join(format!("hrx-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut o = LoadOptions::default();
    o.paths.manager = root.join("mgr");
    o.paths.firmware_dir = root.join("fw");
    std::fs::create_dir_all(&o.paths.manager).unwrap();
    std::fs::create_dir_all(&o.paths.firmware_dir).unwrap();
    std::fs::write(o.paths.manager.join("state"), "operating\n").unwrap();

    let be = MockBackend::new();
    be.poke(0xF800_0108, 30 << 12); // IO PLL at 1 GHz
    let d = Design::load_with(be.clone(), &o).unwrap();
    let v = be.peek(0xF800_0170);
    assert_eq!(((v >> 8) & 0x3f) * ((v >> 20) & 0x3f), 10);
    assert!(matches!(Design::load_with(be.clone(), &o), Err(Error::AlreadyLoaded)));
    drop(d);
    Design::load_with(be, &o).unwrap();
    // A clock failure must release the singleton.
    let mut bad = o.clone();
    bad.apply_clocks = true;
    assert!(Design::load_with(MockBackend::new(), &bad).is_err());
    Design::load_with(MockBackend::new(), &LoadOptions { apply_clocks: false, ..o.clone() }).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
