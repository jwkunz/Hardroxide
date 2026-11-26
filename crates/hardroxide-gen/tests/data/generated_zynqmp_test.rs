//! Run inside the crate generated for the RFSoC 4x2's base overlay: load it with mock registers and a fake
//! `fpga_manager`.
use design_1::{Design, Error, LoadOptions};
use hardroxide_fpga::FpgaPaths;
use hardroxide_mmio::MockBackend;

#[test]
fn loading_programs_sets_ports_and_clocks_and_maps_the_blocks() {
    let root = std::env::temp_dir().join(format!("hrx-zynqmp-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = FpgaPaths {
        manager: root.join("manager"),
        firmware_dir: root.join("firmware"),
        platform_devices: root.join("platform"),
        refuse_bound_pl_drivers: true,
    };
    std::fs::create_dir_all(&paths.manager).unwrap();
    std::fs::create_dir_all(&paths.firmware_dir).unwrap();
    std::fs::write(paths.manager.join("state"), "operating\n").unwrap();

    // The registers as another design left them: both clocks at 250 MHz, every port 32 or 128 bits the other way round.
    let be = MockBackend::new();
    be.poke(0xFF5E_0020, 0x0001_5A00); // I/O PLL, 1.5 GHz
    be.poke(0xFF5E_00C0, 0x0101_0600);
    be.poke(0xFF5E_00C4, 0x0101_0600);
    be.poke(0xFD61_5000, 0x0000_005A);
    be.poke(0xFF41_9000, 0x0000_0200);
    be.poke(0xFD38_0000, 2);

    let options = LoadOptions {
        paths: paths.clone(),
        apply_clocks: true,
    };
    let design = Design::load_with(be.clone(), &options).unwrap();
    let handed = std::fs::read_to_string(paths.manager.join("firmware")).unwrap();
    assert!(handed.starts_with("hardroxide-"), "{handed}");
    // 100 MHz and 300 MHz.
    assert_eq!(be.peek(0xFF5E_00C0), 0x0101_0F00);
    assert_eq!(be.peek(0xFF5E_00C4), 0x0101_0500);
    // Both full-power master ports 128 bits, other bits kept; the low-power one 32.
    assert_eq!(be.peek(0xFD61_5000), 0x0000_0A5A);
    assert_eq!(be.peek(0xFF41_9000), 0);
    // The slave port HP0 (S_AXI_GP2) at 128 bits.
    assert_eq!(be.peek(0xFD38_0000), 0);

    design
        .leds_gpio
        .write(hardroxide_ip::gpio::Channel::One, 5)
        .unwrap();
    assert_eq!(be.peek(0x8004_0000), 5);
    assert!(matches!(
        Design::load_with(be.clone(), &options),
        Err(Error::AlreadyLoaded)
    ));
    std::fs::remove_dir_all(root).unwrap();
}
