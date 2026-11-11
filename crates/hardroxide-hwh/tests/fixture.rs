use hardroxide_hwh::*;

fn xsa() -> Xsa {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/pynq_z2_gpio.xsa"
    );
    Xsa::open(p).expect("open fixture")
}

#[test]
fn parses_design_header() {
    let d = xsa().design;
    assert_eq!(d.vivado_version, "2025.2");
    assert_eq!((d.arch.as_str(), d.device.as_str()), ("zynq", "7z020"));
}

#[test]
fn finds_gpio_with_registers_and_address() {
    let d = xsa().design;
    let g = d.instance("axi_gpio_0").expect("gpio");
    assert_eq!(g.vlnv, "xilinx.com:ip:axi_gpio:2.0");
    assert!(!g.is_ps);
    let blk = &g.address_blocks[0];
    assert_eq!(blk.range, 4096);
    let names: Vec<_> = blk
        .registers
        .iter()
        .map(|r| (r.name.as_str(), r.offset))
        .collect();
    assert_eq!(
        names,
        [
            ("GPIO_DATA", 0),
            ("GPIO_TRI", 4),
            ("GPIO2_DATA", 8),
            ("GPIO2_TRI", 12),
            ("GIER", 0x11c),
            ("IP_IER", 0x128),
            ("IP_ISR", 0x120)
        ]
    );
    let r = d.ranges_of("axi_gpio_0").next().unwrap();
    assert_eq!((r.base, r.high), (0x4120_0000, 0x4120_FFFF));
    assert_eq!(g.parameters["C_GPIO_WIDTH"], "4");
}

#[test]
fn reads_fclk0() {
    let d = xsa().design;
    let c = d.pl_clocks[0];
    assert!(c.enabled && c.reset_enabled);
    assert_eq!(c.freq_hz, 100_000_000);
    assert!(!d.pl_clocks[1].enabled);
}

#[test]
fn bit_header_and_bin() {
    let x = xsa();
    let b = BitFile::parse(&x.bit).unwrap();
    assert!(b.part.starts_with("7z020"), "{}", b.part);
    // Configuration stream starts with 0xFFFFFFFF dummy words then sync 0xAA995566.
    let bin = b.to_bin_swapped().unwrap();
    assert!(bin.len() >= b.payload.len() && bin.len().is_multiple_of(16));
    assert_eq!(&bin[bin.len() - 4..], [0, 0, 0, 0x20]);
    assert!(
        b.payload
            .windows(4)
            .take(64)
            .any(|w| w == [0xAA, 0x99, 0x55, 0x66])
    );
}

#[test]
fn interface_hash_is_stable_and_ignores_clocks() {
    let mut d = xsa().design;
    let h = d.interface_hash();
    assert_eq!(h.len(), 64);
    d.pl_clocks[0].freq_hz = 50_000_000;
    d.vivado_version = "9999".into();
    assert_eq!(d.interface_hash(), h);
    d.memory_ranges[0].base += 0x10000;
    assert_ne!(d.interface_hash(), h);
}

fn open(n: &str) -> Design {
    Xsa::open(format!(
        "{}/../../fixtures/{n}.xsa",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .design
}

#[test]
fn irq_routes_direct_and_through_concat() {
    let hls = open("pynq_z2_hls_vadd");
    assert_eq!(hls.ps7_irq_f2p_bit("vadd_0", "interrupt"), Some(0));
    let dma = open("pynq_z2_dma_loopback");
    assert_eq!(dma.ps7_irq_f2p_bit("axi_dma_0", "mm2s_introut"), Some(0));
    assert_eq!(dma.ps7_irq_f2p_bit("axi_dma_0", "s2mm_introut"), Some(1));
    assert_eq!(dma.ps7_irq_f2p_bit("axi_dma_0", "nonexistent"), None);
    // GPIO's interrupt output is unconnected.
    assert_eq!(
        open("pynq_z2_gpio").ps7_irq_f2p_bit("axi_gpio_0", "ip2intc_irpt"),
        None
    );
}

#[test]
fn irq_routes_through_an_interrupt_controller() {
    use hardroxide_hwh::IrqRoute;
    let d = open("pynq_z2_intc");
    let via = |inst: &str, port: &str, bit| {
        Some(IrqRoute::Intc {
            controller: "axi_intc_0".into(),
            bit,
        })
        .eq(&d.irq_route(inst, port))
    };
    assert!(via("axi_timer_0", "interrupt", 0));
    assert!(via("axi_iic_0", "iic2intc_irpt", 1));
    assert!(via("axi_gpio_0", "ip2intc_irpt", 2));
    // Devices behind the controller have no direct PS7 bit...
    assert_eq!(d.ps7_irq_f2p_bit("axi_timer_0", "interrupt"), None);
    // ...but the controller's own output does.
    assert_eq!(
        d.irq_route("axi_intc_0", "irq"),
        Some(IrqRoute::Ps7 { bit: 0 })
    );
}

#[test]
fn gic_spi_cells_follow_the_trm_table() {
    assert_eq!(zynq7000_f2p_spi_cell(0), Some(29)); // interrupt ID 61
    assert_eq!(zynq7000_f2p_spi_cell(7), Some(36)); // ID 68
    assert_eq!(zynq7000_f2p_spi_cell(8), Some(52)); // ID 84
    assert_eq!(zynq7000_f2p_spi_cell(15), Some(59)); // ID 91
    assert_eq!(zynq7000_f2p_spi_cell(16), None);
}
