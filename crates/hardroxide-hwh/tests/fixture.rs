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
    d.memory_ranges
        .iter_mut()
        .find(|r| r.master_is_ps)
        .unwrap()
        .base += 0x10000;
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

#[test]
fn constructor_parameters_are_part_of_the_interface() {
    // A value compiled into a generated driver constructor must change the
    // interface hash; an unrelated parameter must not.
    let base = open("pynq_z2_peripherals");
    let h0 = base.interface_hash();
    let mut d = base.clone();
    let spi = d
        .instances
        .iter_mut()
        .find(|i| i.name == "axi_quad_spi_0")
        .unwrap();
    spi.parameters.insert("C_FIFO_DEPTH".into(), "256".into());
    assert_ne!(d.interface_hash(), h0, "SPI FIFO depth");
    let mut d = base.clone();
    let t = d
        .instances
        .iter_mut()
        .find(|i| i.name == "axi_timer_0")
        .unwrap();
    for p in &mut t.ports {
        if p.clk_hz.is_some() {
            p.clk_hz = Some(50_000_000);
        }
    }
    assert_ne!(d.interface_hash(), h0, "timer clock");
    let mut d = base.clone();
    let t = d
        .instances
        .iter_mut()
        .find(|i| i.name == "axi_timer_0")
        .unwrap();
    t.parameters
        .insert("C_SOME_UNUSED_PARAM".into(), "1".into());
    assert_eq!(d.interface_hash(), h0, "unrelated parameter");
    let dma = open("pynq_z2_dma_loopback");
    let i = dma.instance("axi_dma_0").unwrap();
    assert_eq!(
        i.driver_config()
            .get("C_SG_LENGTH_WIDTH")
            .map(String::as_str),
        Some("23")
    );
}

#[test]
fn third_party_design_uses_only_ps_visible_addresses() {
    // The PYNQ base overlay has four MicroBlaze subsystems whose private
    // address maps overlap each other and the PS's. Only ps7_0's map says what
    // software on the PS can reach.
    let d = open("real/pynq_base_overlay");
    let ps: Vec<_> = d.host_ranges().collect();
    assert!(
        ps.len() > 10 && ps.len() < 40,
        "{} PS-visible ranges",
        ps.len()
    );
    assert!(
        d.memory_ranges.len() > ps.len(),
        "other masters are parsed too"
    );
    // MicroBlaze-private peripherals have no PS address at all.
    assert_eq!(d.ranges_of("iop_pmoda_timer").count(), 0);
    assert_eq!(
        d.ranges_of("iop_arduino_timers_subsystem_timer_0").count(),
        0
    );
    // PS-visible peripherals keep theirs.
    let leds = d.ranges_of("leds_gpio").next().unwrap();
    assert_eq!((leds.base, leds.master.as_str()), (0x4125_0000, "ps7_0"));
    // No two PS-visible ranges overlap.
    let mut spans: Vec<_> = ps.iter().map(|r| (r.base, r.high, &r.instance)).collect();
    spans.sort();
    for w in spans.windows(2) {
        assert!(w[0].1 < w[1].0, "{} overlaps {}", w[0].2, w[1].2);
    }
}

fn hwh(name: &str) -> Design {
    let text = std::fs::read_to_string(format!(
        "{}/../../fixtures/real/hwh/{name}.hwh",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    hardroxide_hwh::parse_hwh(&text).unwrap()
}

#[test]
fn parses_a_vivado_2019_2_zynq7000_hls_design() {
    let d = hwh("resizer_vivado2019_2_zynq7000");
    assert_eq!(d.arch, "zynq");
    assert!(d.vivado_version.starts_with("2019"), "{}", d.vivado_version);
    assert_eq!(d.ps_clk_hz, 33_333_333); // a PS crystal other than 50 MHz
    assert!(d.instances.iter().any(|i| i.is_hls()));
    assert!(d.host_ranges().count() >= 1);
    assert_eq!(d.interface_hash().len(), 64);
}

#[test]
fn parses_an_ultrascale_plus_design_so_it_can_be_inspected_and_refused() {
    let d = hwh("rfsoc_sam_vivado2020_2_zynqmp");
    assert!(d.arch.starts_with("zynquplus"), "{}", d.arch);
    assert!(d.vivado_version.starts_with("2020"), "{}", d.vivado_version);
    assert!(d.instances.len() > 5);
}

fn open_path(path: &str) -> Design {
    let p = format!("{}/../../fixtures/{path}.xsa", env!("CARGO_MANIFEST_DIR"));
    Xsa::open(p)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .design
}

#[test]
fn reads_the_rfsoc_base_overlays_clocks_and_interrupts() {
    let d = open_path("board/rfsoc4x2_base_overlay");
    assert!(d.is_zynq_ultrascale_plus(), "{}", d.arch);
    assert_eq!(d.ps_clk_hz, 33_333_330); // the design says 33.33333 MHz
    assert_eq!(d.enabled_fclks(), [(0, 99_999_985), (1, 299_999_939)]);
    assert!(d.pl_clocks[0].reset_enabled && !d.pl_clocks[1].reset_enabled);
    // The interrupt controller's output is pl_ps_irq0[0]: GIC ID 121, SPI cell 89.
    assert_eq!(d.ps7_irq_f2p_bit("axi_intc_0", "irq"), Some(0));
    assert_eq!(zynqmp_pl_ps_spi_cell(0), Some(89));
    // The buttons feed input 1 of that controller.
    assert_eq!(
        d.irq_route("btns_gpio", "ip2intc_irpt"),
        Some(IrqRoute::Intc {
            controller: "axi_intc_0".into(),
            bit: 1
        })
    );
    assert!(!open_path("pynq_z2_gpio").is_zynq_ultrascale_plus());
}

#[test]
fn picks_the_designs_handoff_among_several_and_reads_both_interrupt_ports() {
    // This XSA also holds the SmartConnects' handoff files.
    let d = open_path("rfsoc4x2/rfsoc4x2_dma");
    assert!(d.instances.iter().any(|i| i.mod_type == "zynq_ultra_ps_e"));
    let mut bits = Vec::new();
    for i in &d.instances {
        for p in &i.interrupts {
            if let Some(bit) = d.ps7_irq_f2p_bit(&i.name, &p.name) {
                bits.push(bit);
            }
        }
    }
    bits.sort_unstable();
    // pl_ps_irq0[1:0] and pl_ps_irq1[1:0].
    assert_eq!(bits, [0, 1, 8, 9]);
    assert_eq!(zynqmp_pl_ps_spi_cell(8), Some(104));
    assert_eq!(zynqmp_pl_ps_spi_cell(15), Some(111));
    assert_eq!(zynqmp_pl_ps_spi_cell(16), None);
    for name in ["rfsoc4x2_hls_scale", "rfsoc4x2_rf_loopback"] {
        open_path(&format!("rfsoc4x2/{name}"));
    }
}
