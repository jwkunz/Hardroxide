use crate::{Options, names};
use hardroxide_hwh::{Design, Instance, MemRange, Register};
use std::fmt::Write;

pub struct Reg<'a> {
    pub ident: String,
    pub reg: &'a Register,
    pub fields: Vec<String>,
}

pub struct Inst<'a> {
    pub ident: String,
    pub inst: &'a Instance,
    pub ranges: Vec<&'a MemRange>,
    pub regs: Vec<Reg<'a>>,
    /// Rust type of the driver given backend `B`, if curated.
    pub driver: Option<String>,
    /// Generated typed wrapper (HLS IP): its type name.
    pub generated: Option<String>,
}

pub struct Model<'a> {
    pub insts: Vec<Inst<'a>>,
}

/// VLNV (version-less) -> curated driver type path.
fn driver_for(vlnv: &str) -> Option<&'static str> {
    let base = vlnv.rsplit_once(':').map_or(vlnv, |(b, _)| b);
    match base {
        "xilinx.com:ip:axi_gpio" => Some("hardroxide_ip::gpio::AxiGpio"),
        "xilinx.com:ip:axi_dma" => Some("hardroxide_ip::dma::AxiDma"),
        "xilinx.com:ip:axi_timer" => Some("hardroxide_ip::timer::AxiTimer"),
        "xilinx.com:ip:axi_iic" => Some("hardroxide_ip::iic::AxiIic"),
        "xilinx.com:ip:axi_quad_spi" => Some("hardroxide_ip::spi::AxiSpi"),
        "xilinx.com:ip:axi_bram_ctrl" => Some("hardroxide_ip::bram::AxiBram"),
        "xilinx.com:ip:axi_fifo_mm_s" => Some("hardroxide_ip::fifo::AxiFifo"),
        "xilinx.com:ip:axi_uartlite" => Some("hardroxide_ip::uart::AxiUart"),
        "xilinx.com:ip:axi_intc" => Some("hardroxide_ip::intc::AxiIntc"),
        "xilinx.com:user:audio_codec_ctrl" => Some("hardroxide_ip::audio::AudioCodecCtrl"),
        "xilinx.com:ip:axi_vdma" => Some("hardroxide_ip::vdma::AxiVdma"),
        "xilinx.com:ip:v_tc" => Some("hardroxide_ip::vtc::VideoTimingController"),
        "digilentinc.com:ip:axi_dynclk" => Some("hardroxide_ip::dynclk::AxiDynclk"),
        _ => None,
    }
}

fn camel(ident: &str) -> String {
    ident
        .split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

impl<'a> Model<'a> {
    pub fn build(d: &'a Design) -> Self {
        let with_ranges: Vec<&Instance> = d
            .instances
            .iter()
            .filter(|i| !i.is_ps && d.ranges_of(&i.name).next().is_some())
            .collect();
        let idents = names::unique_idents(with_ranges.iter().map(|i| i.name.as_str()));
        let insts = with_ranges
            .into_iter()
            .zip(idents)
            .map(|(inst, ident)| {
                let ranges: Vec<_> = d.ranges_of(&inst.name).collect();
                let blk = inst
                    .address_blocks
                    .iter()
                    .find(|b| b.name == ranges[0].address_block);
                let regs_raw: Vec<&Register> = blk
                    .map(|b| b.registers.iter().collect())
                    .unwrap_or_default();
                let ids = names::unique_idents(regs_raw.iter().map(|r| r.name.as_str()));
                let regs = regs_raw
                    .into_iter()
                    .zip(ids)
                    .map(|(reg, ident)| Reg {
                        ident,
                        fields: names::unique_idents(reg.fields.iter().map(|f| f.name.as_str())),
                        reg,
                    })
                    .collect();
                let (driver, generated) = if inst.is_hls() {
                    let t = camel(&ident);
                    (Some(format!("crate::ips::{t}")), Some(t))
                } else {
                    (driver_for(&inst.vlnv).map(String::from), None)
                };
                Inst {
                    ident,
                    inst,
                    ranges,
                    regs,
                    driver,
                    generated,
                }
            })
            .collect();
        let mut m = Model { insts };
        let mut seen = std::collections::BTreeSet::new();
        for i in &mut m.insts {
            if let Some(t) = &mut i.generated {
                while !seen.insert(t.clone()) {
                    t.push('X');
                }
                i.driver = Some(format!("crate::ips::{t}"));
            }
        }
        m
    }
}

/// A connected interrupt output of an instance.
pub struct Irq {
    pub port: String,
    pub konst: String,
    pub node: String,
    pub f2p_bit: Option<u32>,
    pub gic_spi: Option<u32>,
    /// `(controller, bit)` when the signal feeds an `axi_intc` input.
    pub via_intc: Option<(String, u32)>,
    /// Device-tree trigger type cell (4 = level high, 1 = rising edge).
    pub trigger: u32,
}

/// The PYNQ image of a Zynq UltraScale+ board already has a UIO device on `pl_ps_irq0[0]` (GIC SPI cell 89), a node
/// named `fabric` in its device tree. A design's line there is served by it: a second node could not claim the line.
const IMAGE_UIO_SPI: u32 = 89;
const IMAGE_UIO_NODE: &str = "fabric";

pub fn irq_lines(i: &Inst, d: &Design) -> Vec<Irq> {
    let ports: Vec<_> = i
        .inst
        .interrupts
        .iter()
        .filter(|p| !p.sinks.is_empty())
        .collect();
    let konsts = names::unique_idents(ports.iter().map(|p| p.name.as_str()));
    ports
        .into_iter()
        .zip(konsts)
        .map(|(p, k)| {
            let f2p_bit = d.ps7_irq_f2p_bit(&i.inst.name, &p.name);
            let via_intc = match d.irq_route(&i.inst.name, &p.name) {
                Some(hardroxide_hwh::IrqRoute::Intc { controller, bit }) => Some((controller, bit)),
                _ => None,
            };
            let gic_spi = f2p_bit.and_then(if d.is_zynq_ultrascale_plus() {
                hardroxide_hwh::zynqmp_pl_ps_spi_cell
            } else {
                hardroxide_hwh::zynq7000_f2p_spi_cell
            });
            Irq {
                via_intc,
                port: p.name.clone(),
                konst: format!("IRQ_{}", k.to_uppercase()),
                node: if d.is_zynq_ultrascale_plus() && gic_spi == Some(IMAGE_UIO_SPI) {
                    IMAGE_UIO_NODE.into()
                } else {
                    format!("{}_{}", i.ident, names::snake(&p.name))
                },
                f2p_bit,
                gic_spi,
                trigger: if p.sensitivity.starts_with("EDGE_RISING") {
                    1
                } else {
                    4
                },
            }
        })
        .collect()
}

/// Device-tree overlay exposing each routed PL interrupt as a UIO device.
pub fn interrupts_dtso(m: &Model, d: &Design) -> Option<String> {
    let mut nodes = String::new();
    let mut skipped = String::new();
    for i in &m.insts {
        for l in irq_lines(i, d) {
            if let Some((c, bit)) = &l.via_intc {
                let _ = writeln!(
                    skipped,
                    "// {}.{}: input {bit} of {c}; served by that controller's own interrupt",
                    i.inst.name, l.port
                );
                continue;
            }
            match l.gic_spi {
                Some(IMAGE_UIO_SPI) if d.is_zynq_ultrascale_plus() => {
                    let _ = writeln!(
                        skipped,
                        "// {}.{}: SPI cell {IMAGE_UIO_SPI}, served by the image's own `{IMAGE_UIO_NODE}` UIO node",
                        i.inst.name, l.port
                    );
                }
                Some(spi) => {
                    let r = i.ranges[0];
                    let _ = write!(
                        nodes,
                        "\t\t\t{node}@{base:x} {{\n\t\t\t\tcompatible = \"generic-uio\";\n\t\t\t\treg = <{reg}>;\n\t\t\t\tinterrupt-parent = <&{parent}>;\n\t\t\t\tinterrupts = <0 {spi} {trig}>;\n\t\t\t}};\n",
                        node = l.node,
                        base = r.base,
                        reg = if d.is_zynq_ultrascale_plus() {
                            // Two cells for an address and two for a size at the root of this family's tree.
                            format!(
                                "{:#x} {:#x} {:#x} {:#x}",
                                r.base >> 32,
                                r.base & 0xFFFF_FFFF,
                                r.size() >> 32,
                                r.size() & 0xFFFF_FFFF
                            )
                        } else {
                            format!("{:#x} {:#x}", r.base, r.size())
                        },
                        parent = if d.is_zynq_ultrascale_plus() {
                            "gic"
                        } else {
                            "intc"
                        },
                        trig = l.trigger
                    );
                }
                None => {
                    let vector = if d.is_zynq_ultrascale_plus() {
                        "pl_ps_irq1:pl_ps_irq0"
                    } else {
                        "PS7 IRQ_F2P[15:0]"
                    };
                    let _ = writeln!(
                        skipped,
                        "// skipped {}.{}: not traceable to {vector}",
                        i.inst.name, l.port
                    );
                }
            }
        }
    }
    if nodes.is_empty() {
        return None;
    }
    let notes = if d.is_zynq_ultrascale_plus() {
        "// Requires: a kernel with uio_pdrv_genirq and `uio_pdrv_genirq.of_id=generic-uio`\n// on the command line, and a base device tree built with symbols (for &gic), as the\n// PYNQ image of the RFSoC 4x2 has.\n// Zynq UltraScale+ mapping: pl_ps_irq0 -> SPI cell 89..=96, pl_ps_irq1 -> 104..=111.\n// UNVERIFIED on hardware; see docs/decisions/0005-ultrascale-plus.md.\n"
    } else {
        "// Requires: a kernel with uio_pdrv_genirq and `uio_pdrv_genirq.of_id=generic-uio`\n// on the command line, and a base device tree built with symbols (for &intc).\n// Zynq-7000 mapping: IRQ_F2P[7:0] -> SPI cell 29..=36, [15:8] -> 52..=59.\n// UNVERIFIED on hardware; see docs/08-verification-gaps.md (G11).\n"
    };
    // The root of this family's tree has two cells for an address and two for a size; saying so again lets the overlay
    // be compiled on its own without warnings about the nodes' `reg`.
    let cells = if d.is_zynq_ultrascale_plus() {
        "\t\t\t#address-cells = <2>;\n\t\t\t#size-cells = <2>;\n"
    } else {
        ""
    };
    Some(format!(
        "// @generated by hardroxide. Do not edit; regenerate from the XSA.\n//\n// Exposes each PL interrupt as a UIO device (one node per interrupt line).\n{notes}{skipped}/dts-v1/;\n/plugin/;\n\n/ {{\n\tfragment@0 {{\n\t\ttarget-path = \"/\";\n\t\t__overlay__ {{\n{cells}{nodes}\t\t}};\n\t}};\n}};\n"
    ))
}

/// Lists every interrupt line and opens its UIO device (checks the overlay).
pub fn irq_example(m: &Model, d: &Design, crate_name: &str) -> Option<String> {
    if !m.insts.iter().any(|i| !irq_lines(i, d).is_empty()) {
        return None;
    }
    let lib = crate_name.replace('-', "_");
    let mut s = String::from(HEADER);
    let _ = writeln!(
        s,
        "//! Lists every interrupt line of the design and tries to open its UIO device,"
    );
    let _ = writeln!(
        s,
        "//! which checks that the overlay (interrupts.dtso) is applied. Does not wait.\n"
    );
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    use {lib}::pac;");
    let _ = writeln!(s, "    let mut missing = 0;");
    for i in &m.insts {
        if !irq_lines(i, d).is_empty() {
            let _ = writeln!(s, "    for line in pac::{}::IRQS {{", i.ident);
            let _ = writeln!(
                s,
                "        if let Some((c, bit)) = line.via_intc {{\n            println!(\"{{}}.{{}}: via {{c}} input {{bit}}\", line.instance, line.port);\n            continue;\n        }}"
            );
            let _ = writeln!(s, "        match line.open() {{");
            let bit = if d.is_zynq_ultrascale_plus() {
                "pl_ps_irq bit"
            } else {
                "IRQ_F2P bit"
            };
            let _ = writeln!(
                s,
                "            Ok(_) => println!(\"{{}}.{{}}: UIO ok ({bit} {{:?}}, SPI cell {{:?}})\", line.instance, line.port, line.f2p_bit, line.gic_spi),"
            );
            let _ = writeln!(
                s,
                "            Err(e) => {{\n                missing += 1;\n                println!(\"{{}}.{{}}: {{e}}\", line.instance, line.port)\n            }}"
            );
            let _ = writeln!(s, "        }}\n    }}");
        }
    }
    let _ = writeln!(
        s,
        "    if missing > 0 {{\n        std::process::exit(1);\n    }}\n}}"
    );
    Some(s)
}

pub(crate) const HEADER: &str =
    "// @generated by hardroxide. Do not edit; regenerate from the XSA.\n";

pub fn cargo_toml(name: &str, opts: &Options, d: &Design) -> String {
    // Zynq UltraScale+ support begins with the 3.0 crates.
    let version = if d.is_zynq_ultrascale_plus() {
        "3"
    } else {
        "2"
    };
    let dep = |c: &str| match &opts.hardroxide_path {
        Some(root) => format!(
            "{c} = {{ path = \"{}\" }}",
            root.join("crates").join(c).display()
        ),
        None => format!("{c} = \"{version}\""),
    };
    format!(
        "# @generated by hardroxide.\n[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\n# Runtime bitstream loading from an XSA (development only).\ndev-bitstream = [\"dep:hardroxide-hwh\"]\n\n[dependencies]\n{}\n{}\n{}\n{}\n{}\n{}\nthiserror = \"2\"\n\n[dev-dependencies]\nembedded-hal = \"1\"\n",
        dep("hardroxide-mmio"),
        dep("hardroxide-fpga"),
        dep("hardroxide-ip"),
        dep("hardroxide-irq"),
        dep("hardroxide-dma"),
        dep("hardroxide-hwh")
            .replace(" }", ", optional = true }")
            .replace(
                &format!("= \"{version}\""),
                &format!("= {{ version = \"{version}\", optional = true }}")
            ),
    )
}

pub fn pac_rs(m: &Model, d: &Design) -> String {
    let mut s = String::from(HEADER);
    s.push_str("//! Register layer: addresses, offsets and field masks per IP instance.\n\n");
    for i in &m.insts {
        let r0 = i.ranges[0];
        let _ = writeln!(s, "/// `{}` (`{}`).", i.inst.name, i.inst.vlnv);
        let _ = writeln!(s, "pub mod {} {{", i.ident);
        let _ = writeln!(s, "    pub const VLNV: &str = {:?};", i.inst.vlnv);
        let _ = writeln!(s, "    pub const BASE_ADDRESS: u64 = {:#x};", r0.base);
        let _ = writeln!(s, "    pub const SIZE: usize = {:#x};", r0.size());
        let irqs: Vec<String> = i
            .inst
            .interrupts
            .iter()
            .filter(|p| !p.sinks.is_empty())
            .map(|p| {
                format!(
                    "({:?}, {:?})",
                    p.name,
                    d.ps7_irq_f2p_bit(&i.inst.name, &p.name)
                )
            })
            .collect();
        // One `IrqLine` per connected interrupt, plus an `IRQS` slice.
        let lines = irq_lines(i, d);
        for l in &lines {
            let _ = writeln!(
                s,
                "    /// Interrupt `{}` (PS7 `IRQ_F2P` bit {:?}); open with `.open()`.",
                l.port, l.f2p_bit
            );
            let _ = writeln!(
                s,
                "    pub const {}: hardroxide_irq::IrqLine = hardroxide_irq::IrqLine {{ instance: {:?}, port: {:?}, f2p_bit: {:?}, gic_spi: {:?}, dt_node: {:?}, base: {:#x}, via_intc: {:?}, edge: {} }};",
                l.konst,
                i.inst.name,
                l.port,
                l.f2p_bit,
                l.gic_spi,
                l.node,
                i.ranges[0].base,
                l.via_intc.as_ref().map(|(c, b)| (c.as_str(), *b)),
                l.trigger == 1
            );
        }
        let names: Vec<_> = lines.iter().map(|l| l.konst.as_str()).collect();
        let _ = writeln!(
            s,
            "    pub const IRQS: &[hardroxide_irq::IrqLine] = &[{}];",
            names.join(", ")
        );
        let _ = writeln!(
            s,
            "    /// Connected interrupt outputs: `(port, PS7 IRQ_F2P bit if routed directly)`."
        );
        let _ = writeln!(
            s,
            "    pub const INTERRUPTS: &[(&str, Option<u32>)] = &[{}];",
            irqs.join(", ")
        );
        for r in &i.regs {
            let _ = writeln!(s, "\n    /// {}", one_line(&r.reg.description));
            let _ = writeln!(s, "    pub mod {} {{", r.ident);
            let _ = writeln!(s, "        pub const OFFSET: usize = {:#x};", r.reg.offset);
            let _ = writeln!(
                s,
                "        pub const RESET_VALUE: u32 = {:#x};",
                r.reg.reset_value as u32
            );
            for (f, fi) in r.reg.fields.iter().zip(&r.fields) {
                let mask: u64 = if f.bit_width >= 32 {
                    0xffff_ffff
                } else {
                    ((1u64 << f.bit_width) - 1) << f.bit_offset
                };
                let _ = writeln!(s, "\n        /// {}", one_line(&f.description));
                let _ = writeln!(s, "        pub mod {fi} {{");
                let _ = writeln!(s, "            pub const SHIFT: u32 = {};", f.bit_offset);
                let _ = writeln!(s, "            pub const WIDTH: u32 = {};", f.bit_width);
                let _ = writeln!(s, "            pub const MASK: u32 = {:#x};", mask as u32);
                let _ = writeln!(s, "        }}");
            }
            let _ = writeln!(s, "    }}");
        }
        let _ = writeln!(s, "}}\n");
    }
    if d.is_zynq_ultrascale_plus() {
        // The same 16 bits, named as this family names them: `pl_ps_irq1:pl_ps_irq0`.
        s = s
            .replace("PS7 `IRQ_F2P` bit", "`pl_ps_irq1:pl_ps_irq0` bit")
            .replace("PS7 IRQ_F2P bit", "pl_ps_irq1:pl_ps_irq0 bit");
    }
    s
}

fn one_line(d: &str) -> String {
    d.lines().next().unwrap_or("").trim().replace("*/", "* /")
}

pub fn lib_rs(m: &Model, d: &Design) -> String {
    let mut s = String::from(HEADER);
    let _ = writeln!(
        s,
        "//! HAL for block design `{}` ({}, Vivado {}).\n",
        d.name, d.device, d.vivado_version
    );
    let fields: String = m
        .insts
        .iter()
        .map(|i| match &i.driver {
            Some(t) => format!("    pub {}: {t}<B::Region>,\n", i.ident),
            None => format!("    /// No curated driver for `{}`: raw register window.\n    pub {}: B::Region,\n", i.inst.vlnv, i.ident),
        })
        .collect();
    let inits: String = m
        .insts
        .iter()
        .map(|i| {
            let map = format!(
                "backend.map(pac::{0}::BASE_ADDRESS, pac::{0}::SIZE)?",
                i.ident
            );
            match &i.driver {
                Some(t) => format!("            {}: {},\n", i.ident, field_ctor(i, d, t, &map)),
                None => format!("            {}: {map},\n", i.ident),
            }
        })
        .collect();
    let mods = if m.insts.iter().any(|i| i.generated.is_some()) {
        "pub mod ips;\n"
    } else {
        ""
    };
    let family = if d.is_zynq_ultrascale_plus() {
        &ZYNQMP_LIB
    } else {
        &ZYNQ_LIB
    };
    let mut lib = LIB_TEMPLATE.to_string();
    for (marker, text) in family {
        assert!(lib.contains(marker), "the template has no {marker}");
        lib = lib.replace(marker, text);
    }
    s.push_str(
        &lib.replace("//MODS//", mods)
            .replace("//FIELDS//", &fields)
            .replace("//INITS//", &inits),
    );
    s
}

/// What differs between the families in the Linux crate's `lib.rs`, as `(marker in the template, text)`.
type Family = [(&'static str, &'static str); 6];

const ZYNQ_LIB: Family = [
    ("//PORT_STATIC//\n", ""),
    (
        "//CLOCKS_DOC//",
        "/// Apply the FCLK targets through the Zynq SLCR before mapping. Disable\n    /// if the clocks are already configured.",
    ),
    ("//EMBEDDED_PORTS//", ""),
    ("//XSA_PORTS//", ""),
    ("//PORTS_PARAMETER//", ""),
    (
        "//PROGRAM//",
        "hardroxide_fpga::load_bitstream(&opts.paths, bin, sha)?;\n            if opts.apply_clocks {\n                let slcr = backend.map(hardroxide_fpga::slcr::SLCR_BASE, hardroxide_fpga::slcr::SLCR_SIZE)?;\n                cfg.apply(&slcr)?;\n            }",
    ),
];

const ZYNQMP_LIB: Family = [
    (
        "//PORT_STATIC//\n",
        "use hardroxide_fpga::zynqmp::PortConfig;\n\nstatic PORTS_CFG: &str = include_str!(\"../bitstream/ports.cfg\");\n\n/// The widths the design gives the ports between the processing system and the PL.\npub fn embedded_port_config() -> PortConfig {\n    PortConfig::parse(PORTS_CFG).expect(\"embedded bitstream/ports.cfg is malformed\")\n}\n",
    ),
    (
        "//CLOCKS_DOC//",
        "/// Set the PL clocks to the design's targets before mapping. Disable if\n    /// they are already configured.",
    ),
    ("//EMBEDDED_PORTS//", ", &embedded_port_config()"),
    (
        "//XSA_PORTS//",
        ", &PortConfig {\n            widths: xsa\n                .design\n                .axi_port_widths()\n                .into_iter()\n                .filter_map(|(master, n, bits)| {\n                    use hardroxide_fpga::zynqmp::{Port, Width};\n                    let port = if master { Port::Master(n) } else { Port::Slave(n) };\n                    Some((port, Width::from_bits(bits)?))\n                })\n                .collect(),\n        }",
    ),
    ("//PORTS_PARAMETER//", "\n        ports: &PortConfig,"),
    (
        "//PROGRAM//",
        "hardroxide_fpga::zynqmp::load_bitstream(&opts.paths, bin, sha)?;\n            // The boot loader set the ports up for another design: give them this one's widths.\n            ports.apply(&backend)?;\n            if opts.apply_clocks {\n                use hardroxide_fpga::zynqmp as fpga;\n                let crl = backend.map(fpga::CRL_APB_BASE, fpga::CRL_APB_SIZE)?;\n                let crf = backend.map(fpga::CRF_APB_BASE, fpga::CRF_APB_SIZE)?;\n                for &(index, hz) in &cfg.fclks {\n                    fpga::set_pl_clock(&crl, &crf, cfg.ps_clk_hz, index, hz)?;\n                }\n            }",
    ),
];

/// The constructor for the field of instance `i`, with what its family needs: on Zynq UltraScale+, where memory
/// beyond 32 bits exists, a DMA engine is told how far it reaches (part of the interface hash, like the other
/// constructor values).
pub(crate) fn field_ctor(i: &Inst, d: &Design, driver: &str, map: &str) -> String {
    let made = ctor(i, driver, map);
    match d.dma_address_width(&i.inst.name) {
        Some(bits) => format!("{made}.with_address_width({bits})"),
        None => made,
    }
}

/// Constructor expression for a driver, with HWH-derived arguments.
pub(crate) fn ctor(i: &Inst, driver: &str, map: &str) -> String {
    // Only `driver_config` values may be used here: they are covered by the
    // interface hash (see hardroxide_hwh::Instance::driver_config).
    let cfg = i.inst.driver_config();
    let p = |k: &str, d: &str| cfg.get(k).cloned().unwrap_or_else(|| d.into());
    match driver.rsplit("::").next().unwrap_or("") {
        "AxiDma" => format!(
            "{driver}::with_length_width({map}, {})",
            p("C_SG_LENGTH_WIDTH", "14")
        ),
        "AxiTimer" => format!("{driver}::new({map}, {})", p("axi_clk_hz", "0")),
        "AxiSpi" => {
            // C_FIFO_EXIST=0 means a single-entry data register.
            let depth = if p("C_FIFO_EXIST", "1") == "0" {
                "1".to_string()
            } else {
                p("C_FIFO_DEPTH", "16")
            };
            format!("{driver}::new({map}, {depth}, {})", p("C_NUM_SS_BITS", "1"))
        }
        _ => format!("{driver}::new({map})"),
    }
}

const LIB_TEMPLATE: &str = r#"pub mod pac;
//MODS//
use hardroxide_fpga::{FpgaError, FpgaPaths, LoadConfig};
use hardroxide_mmio::{DevMem, MmioBackend, MmioError};
use std::sync::atomic::{AtomicBool, Ordering};

/// Hash of the hardware interface this crate was generated for. Changes only
/// on regeneration (`interface.sha256`).
pub const INTERFACE_HASH: &str = include_str!("../interface.sha256");
/// SHA-256 of the embedded bitstream (`bitstream/design.sha256`). Replaced by
/// `hardroxide update` without touching Rust sources.
pub const BITSTREAM_SHA256: &str = include_str!("../bitstream/design.sha256");

static BITSTREAM: &[u8] = include_bytes!("../bitstream/design.bin");
static LOAD_CFG: &str = include_str!("../bitstream/load.cfg");
static LOADED: AtomicBool = AtomicBool::new(false);
//PORT_STATIC//

/// The embedded load configuration (PS clock and FCLK targets).
pub fn embedded_load_config() -> LoadConfig {
    LoadConfig::parse(LOAD_CFG).expect("embedded bitstream/load.cfg is malformed")
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a Design is already loaded in this process")]
    AlreadyLoaded,
    #[error(transparent)]
    Fpga(#[from] FpgaError),
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[cfg(feature = "dev-bitstream")]
    #[error(transparent)]
    Xsa(#[from] hardroxide_hwh::Error),
    #[cfg(feature = "dev-bitstream")]
    #[error("XSA interface differs from the compiled one ({found} != {INTERFACE_HASH}); regenerate the crate")]
    InterfaceMismatch { found: String },
}

#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub paths: FpgaPaths,
    //CLOCKS_DOC//
    pub apply_clocks: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self { paths: FpgaPaths::default(), apply_clocks: true }
    }
}

/// The programmed PL. Only one can exist per process (via `load*`), so two
/// handles never alias the same registers. Dropping it allows a reload.
pub struct Design<B: MmioBackend> {
//FIELDS//    guard: bool,
}

impl Design<DevMem> {
    /// Program the PL, set clocks, and map every IP through `/dev/mem`.
    pub fn load() -> Result<Self, Error> {
        Self::load_with(DevMem::new(), &LoadOptions::default())
    }
}

impl<B: MmioBackend> Design<B> {
    /// Like [`Design::load`] with a custom MMIO backend and options.
    pub fn load_with(backend: B, opts: &LoadOptions) -> Result<Self, Error> {
        Self::load_image(backend, opts, BITSTREAM, BITSTREAM_SHA256, &embedded_load_config()//EMBEDDED_PORTS//)
    }

    /// Development aid: program a bitstream from `xsa` at runtime instead of
    /// the embedded one, provided its interface hash equals the compiled one.
    /// Release builds should not enable `dev-bitstream`.
    #[cfg(feature = "dev-bitstream")]
    pub fn load_with_bitstream(
        xsa: impl AsRef<std::path::Path>,
        backend: B,
        opts: &LoadOptions,
    ) -> Result<Self, Error> {
        let xsa = hardroxide_hwh::Xsa::open(xsa)?;
        let found = xsa.design.interface_hash();
        if found != INTERFACE_HASH {
            return Err(Error::InterfaceMismatch { found });
        }
        let bin = xsa.bin()?;
        let sha = hardroxide_fpga::sha256_hex(&bin);
        let cfg = LoadConfig { ps_clk_hz: xsa.design.ps_clk_hz, fclks: xsa.design.enabled_fclks() };
        Self::load_image(backend, opts, &bin, &sha, &cfg//XSA_PORTS//)
    }

    fn load_image(
        backend: B,
        opts: &LoadOptions,
        bin: &[u8],
        sha: &str,
        cfg: &LoadConfig,//PORTS_PARAMETER//
    ) -> Result<Self, Error> {
        if LOADED.swap(true, Ordering::SeqCst) {
            return Err(Error::AlreadyLoaded);
        }
        let r = (|| {
            //PROGRAM//
            Self::map(&backend, true)
        })();
        if r.is_err() {
            LOADED.store(false, Ordering::SeqCst);
        }
        r
    }

    /// Map the IP without programming the PL or taking the process-wide
    /// singleton. For tests with a mock backend, or when the PL was
    /// programmed by other means. The caller must avoid aliasing.
    pub fn map_only(backend: B) -> Result<Self, Error> {
        Self::map(&backend, false)
    }

    fn map(backend: &B, guard: bool) -> Result<Self, Error> {
        Ok(Self {
//INITS//            guard,
        })
    }
}

impl<B: MmioBackend> Drop for Design<B> {
    fn drop(&mut self) {
        if self.guard {
            LOADED.store(false, Ordering::SeqCst);
        }
    }
}
"#;

/// Hardware smoke-test example for AXI GPIO. With several GPIO instances the
/// name is the first argument; the default is the first one that has output pins
/// (so a design's LEDs are chosen over its buttons).
pub fn gpio_example(m: &Model, crate_name: &str) -> Option<String> {
    let mut all = all_with(m, "::AxiGpio");
    if all.is_empty() {
        return None;
    }
    let param = |i: &Inst, k: &str, d: &str| {
        i.inst
            .parameters
            .get(k)
            .cloned()
            .unwrap_or_else(|| d.into())
    };
    let has_out = |i: &Inst| param(i, "C_ALL_INPUTS", "0") != "1";
    all.sort_by_key(|i| !has_out(i)); // stable: outputs first, address order kept
    let lib = crate_name.replace('-', "_");
    let mut s = String::from(HEADER);
    let _ = writeln!(
        s,
        "//! Hardware smoke test for AXI GPIO: if the instance has output pins, walks a"
    );
    let _ = writeln!(
        s,
        "//! one-hot pattern over channel 1; otherwise reads channel 1. Channel 2, if it"
    );
    let _ = writeln!(
        s,
        "//! is an input, is printed. Run as root: `cargo run --example gpio_smoke [instance]`.\n"
    );
    let _ = writeln!(
        s,
        "use hardroxide_ip::gpio::Channel;\nuse std::{{thread::sleep, time::Duration}};\n"
    );
    let _ = writeln!(s, "fn main() -> Result<(), Box<dyn std::error::Error>> {{");
    let _ = writeln!(s, "    let d = {lib}::Design::load()?;");
    // (instance, width of channel 1, channel 1 has outputs, channel 2 is an input)
    let tuple = |i: &Inst| {
        let w1: u32 = param(i, "C_GPIO_WIDTH", "32")
            .parse()
            .unwrap_or(32)
            .clamp(1, 32);
        let in2 = param(i, "C_IS_DUAL", "0") == "1" && param(i, "C_ALL_INPUTS_2", "0") == "1";
        format!("(&d.{}, {w1}u32, {}, {in2})", i.ident, has_out(i))
    };
    if all.len() == 1 {
        let _ = writeln!(
            s,
            "    // (gpio, width of channel 1, has outputs, channel 2 is an input)"
        );
        let _ = writeln!(s, "    let (gpio, w1, out1, in2) = {};", tuple(all[0]));
    } else {
        let list = all
            .iter()
            .map(|i| i.inst.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            s,
            "    // Several instances: pass the name as the first argument. Default: {}.\n    // Available: {list}\n    // (gpio, width of channel 1, has outputs, channel 2 is an input)",
            all[0].inst.name
        );
        let _ = writeln!(
            s,
            "    let (gpio, w1, out1, in2) = match std::env::args().nth(1).as_deref() {{"
        );
        for (k, i) in all.iter().enumerate() {
            let pat = if i.inst.name == i.ident {
                format!("Some(\"{}\")", i.ident)
            } else {
                format!("Some(\"{}\") | Some(\"{}\")", i.inst.name, i.ident)
            };
            let pat = if k == 0 { format!("None | {pat}") } else { pat };
            let _ = writeln!(s, "        {pat} => {},", tuple(i));
        }
        let _ = writeln!(
            s,
            "        Some(other) => {{\n            eprintln!(\"unknown instance `{{other}}`; choose one of: {list}\");\n            std::process::exit(2);\n        }}\n    }};"
        );
    }
    let _ = writeln!(
        s,
        "    if out1 {{\n        gpio.set_input_mask(Channel::One, 0)?; // all outputs\n        for step in 0..w1 * 2 {{\n            gpio.write(Channel::One, 1 << (step % w1))?;\n            if in2 {{\n                println!(\"channel 2 inputs: {{:#x}}\", gpio.read(Channel::Two)?);\n            }}\n            sleep(Duration::from_millis(250));\n        }}\n        gpio.write(Channel::One, 0)?;\n    }} else {{\n        println!(\"channel 1: {{:#x}}\", gpio.read(Channel::One)?);\n    }}\n    Ok(())\n}}"
    );
    Some(s)
}

/// Typed wrappers for HLS instances, from their HWH register maps.
pub fn ips_rs(m: &Model) -> Option<String> {
    let hls: Vec<_> = m.insts.iter().filter(|i| i.generated.is_some()).collect();
    if hls.is_empty() {
        return None;
    }
    let mut s = String::from(HEADER);
    s.push_str("//! Typed accessors for HLS IP. Control (`ap_start` etc.) comes from\n//! `HlsIp` via `Deref`; user registers are the named methods below.\n\n");
    s.push_str("use crate::pac;\nuse hardroxide_ip::hls::HlsIp;\nuse hardroxide_mmio::{Mmio, MmioError};\nuse core::ops::Deref;\n");
    for i in hls {
        let t = i.generated.as_ref().unwrap();
        let _ = writeln!(s, "\n/// `{}` (`{}`).", i.inst.name, i.inst.vlnv);
        let _ = writeln!(s, "pub struct {t}<R: Mmio>(HlsIp<R>);\n");
        let _ = writeln!(s, "impl<R: Mmio> {t}<R> {{");
        let _ = writeln!(
            s,
            "    pub fn new(regs: R) -> Self {{\n        Self(HlsIp::new(regs))\n    }}"
        );
        // 64-bit pointer arguments (m_axi `offset=slave`): two consecutive
        // registers whose field descriptions read "Bit 31 to 0 of X" and
        // "Bit 63 to 32 of X".
        let mut in_pair = std::collections::BTreeSet::new();
        for w in i.regs.windows(2) {
            let desc = |r: &Reg| r.reg.fields.first().map(|f| f.description.clone());
            let (Some(lo), Some(hi)) = (desc(&w[0]), desc(&w[1])) else {
                continue;
            };
            let Some(arg) = lo.strip_prefix("Bit 31 to 0 of ") else {
                continue;
            };
            if hi != format!("Bit 63 to 32 of {arg}") || w[1].reg.offset != w[0].reg.offset + 4 {
                continue;
            }
            let id = names::snake(arg);
            in_pair.insert(w[0].reg.offset);
            in_pair.insert(w[1].reg.offset);
            let _ = writeln!(
                s,
                "\n    /// Set the 64-bit address argument `{arg}` (offsets {:#x}/{:#x}).\n    pub fn set_{id}_addr(&self, addr: u64) -> Result<(), MmioError> {{\n        self.0.regs().write32(pac::{inst}::{lo}::OFFSET, addr as u32)?;\n        self.0.regs().write32(pac::{inst}::{hi}::OFFSET, (addr >> 32) as u32)\n    }}",
                w[0].reg.offset,
                w[1].reg.offset,
                inst = i.ident,
                lo = w[0].ident,
                hi = w[1].ident
            );
            let _ = writeln!(
                s,
                "\n    /// Read back the 64-bit address argument `{arg}`.\n    pub fn {id}_addr(&self) -> Result<u64, MmioError> {{\n        let lo = self.0.regs().read32(pac::{inst}::{lo}::OFFSET)? as u64;\n        let hi = self.0.regs().read32(pac::{inst}::{hi}::OFFSET)? as u64;\n        Ok(hi << 32 | lo)\n    }}",
                inst = i.ident,
                lo = w[0].ident,
                hi = w[1].ident
            );
            let _ = writeln!(
                s,
                "\n    /// Point `{arg}` at a DMA buffer (its physical address). The caller must\n    /// keep the buffer alive and unmodified by the CPU while the core runs.\n    pub fn set_{id}_buffer(&self, buf: &impl DmaBuffer) -> Result<(), MmioError> {{\n        self.set_{id}_addr(buf.phys_addr())\n    }}"
            );
        }
        for r in i
            .regs
            .iter()
            .filter(|r| r.reg.offset >= 0x10 && !in_pair.contains(&r.reg.offset))
        {
            let readable = r.reg.access != hardroxide_hwh::Access::WriteOnly;
            let writable = r.reg.access != hardroxide_hwh::Access::ReadOnly;
            if readable {
                let _ = writeln!(
                    s,
                    "\n    /// Read `{}` (offset {:#x}).",
                    r.reg.name, r.reg.offset
                );
                let _ = writeln!(
                    s,
                    "    pub fn {}(&self) -> Result<u32, MmioError> {{\n        self.0.regs().read32(pac::{}::{}::OFFSET)\n    }}",
                    r.ident, i.ident, r.ident
                );
            }
            if writable {
                let _ = writeln!(
                    s,
                    "\n    /// Write `{}` (offset {:#x}).",
                    r.reg.name, r.reg.offset
                );
                let _ = writeln!(
                    s,
                    "    pub fn set_{}(&self, value: u32) -> Result<(), MmioError> {{\n        self.0.regs().write32(pac::{}::{}::OFFSET, value)\n    }}",
                    r.ident, i.ident, r.ident
                );
            }
        }
        let _ = writeln!(s, "}}\n");
        let _ = writeln!(
            s,
            "impl<R: Mmio> Deref for {t}<R> {{\n    type Target = HlsIp<R>;\n    fn deref(&self) -> &HlsIp<R> {{\n        &self.0\n    }}\n}}"
        );
    }
    if s.contains("&impl DmaBuffer") {
        s = s.replacen(
            "use crate::pac;\n",
            "use crate::pac;\nuse hardroxide_dma::DmaBuffer;\n",
            1,
        );
    }
    Some(s)
}

/// Hardware smoke test for the first HLS instance: non-destructive status read.
pub fn hls_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = m.insts.iter().find(|i| i.generated.is_some())?;
    let lib = crate_name.replace('-', "_");
    let mut s = String::from(HEADER);
    let _ = writeln!(
        s,
        "//! Hardware smoke test for `{}`: prints the control block and every",
        i.inst.name
    );
    let _ = writeln!(
        s,
        "//! readable user register. Does not start the core. Run as root on the board.\n"
    );
    let _ = writeln!(s, "fn main() -> Result<(), Box<dyn std::error::Error>> {{");
    let _ = writeln!(s, "    let d = {lib}::Design::load()?;");
    let _ = writeln!(s, "    let ip = &d.{};", i.ident);
    let _ = writeln!(
        s,
        "    println!(\"idle={{}} ready={{}} done={{}}\", ip.is_idle()?, ip.is_ready()?, ip.is_done()?);"
    );
    for r in i
        .regs
        .iter()
        .filter(|r| r.reg.offset >= 0x10 && r.reg.access != hardroxide_hwh::Access::WriteOnly)
    {
        let _ = writeln!(
            s,
            "    println!(\"{} = {{:#x}}\", ip.{}()?);",
            r.reg.name, r.ident
        );
    }
    let _ = writeln!(s, "    Ok(())\n}}");
    Some(s)
}

/// Loopback smoke test for the first AXI DMA (assumes MM2S wired back to S2MM).
pub fn dma_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = m
        .insts
        .iter()
        .find(|i| i.driver.as_deref() == Some("hardroxide_ip::dma::AxiDma"))?;
    let lib = crate_name.replace('-', "_");
    let mut s = String::from(HEADER);
    let _ = writeln!(
        s,
        "//! Hardware smoke test for `{}`: sends a pattern through MM2S and",
        i.inst.name
    );
    let _ = writeln!(
        s,
        "//! expects it back on S2MM. Requires the stream ports wired as a loopback"
    );
    let _ = writeln!(
        s,
        "//! and two `u-dma-buf` devices (`/dev/udmabuf0`, `/dev/udmabuf1`) of at"
    );
    let _ = writeln!(s, "//! least 4 KiB. Run as root on the board.\n");
    let _ = writeln!(
        s,
        "use hardroxide_dma::{{DmaBuffer, Udmabuf}};\nuse hardroxide_ip::dma::Direction;\nuse std::time::Duration;\n"
    );
    let _ = writeln!(s, "fn main() -> Result<(), Box<dyn std::error::Error>> {{");
    let _ = writeln!(s, "    let d = {lib}::Design::load()?;");
    s.push_str(&select("dma", &all_with(m, "::AxiDma"), false));
    let _ = writeln!(s, "    dma.reset()?;");
    let _ = writeln!(
        s,
        "    let mut tx = Udmabuf::open(0)?;\n    let rx = Udmabuf::open(1)?;"
    );
    let _ = writeln!(s, "    let n = 4096.min(tx.len()).min(rx.len());");
    let _ = writeln!(
        s,
        "    for (i, b) in tx.as_mut_slice()[..n].iter_mut().enumerate() {{\n        *b = (i * 7 + 1) as u8;\n    }}"
    );
    let _ = writeln!(s, "    // Arm the receiver first, then send.");
    let _ = writeln!(
        s,
        "    let rx_t = dma.start(Direction::StreamToMem, rx, n).map_err(|e| e.error)?;"
    );
    let _ = writeln!(
        s,
        "    let tx_t = dma.start(Direction::MemToStream, tx, n).map_err(|e| e.error)?;"
    );
    let _ = writeln!(s, "    let tx = tx_t.wait(Duration::from_secs(1))?;");
    let _ = writeln!(s, "    let rx = rx_t.wait(Duration::from_secs(1))?;");
    let _ = writeln!(
        s,
        "    assert_eq!(&tx.as_slice()[..n], &rx.as_slice()[..n], \"loopback data mismatch\");"
    );
    let _ = writeln!(
        s,
        "    println!(\"DMA loopback OK: {{n}} bytes\");\n    Ok(())\n}}"
    );
    Some(s)
}

/// Every instance whose curated driver path ends with `suffix`, in address order.
pub(crate) fn all_with<'a>(m: &'a Model, suffix: &str) -> Vec<&'a Inst<'a>> {
    m.insts
        .iter()
        .filter(|i| i.driver.as_deref().is_some_and(|d| d.ends_with(suffix)))
        .collect()
}

/// Rust that binds `var` to one of `insts` chosen by name from the first
/// command-line argument (default: the first). `mutable` borrows `&mut`.
fn select(var: &str, insts: &[&Inst], mutable: bool) -> String {
    let r = if mutable { "&mut d." } else { "&d." };
    if insts.len() == 1 {
        return format!("    let {var} = {r}{};\n", insts[0].ident);
    }
    let names: Vec<&str> = insts.iter().map(|i| i.inst.name.as_str()).collect();
    let list = names.join(", ");
    let mut arms = String::new();
    for (k, i) in insts.iter().enumerate() {
        let pat = if i.inst.name == i.ident {
            format!("Some(\"{}\")", i.ident)
        } else {
            format!("Some(\"{}\") | Some(\"{}\")", i.inst.name, i.ident)
        };
        let pat = if k == 0 { format!("None | {pat}") } else { pat };
        let _ = writeln!(arms, "        {pat} => {r}{},", i.ident);
    }
    format!(
        "    // Several instances: pass the name as the first argument. Default: {}.\n    // Available: {list}\n    let {var} = match std::env::args().nth(1).as_deref() {{\n{arms}        Some(other) => {{\n            eprintln!(\"unknown instance `{{other}}`; choose one of: {list}\");\n            std::process::exit(2);\n        }}\n    }};\n",
        names[0]
    )
}

pub(crate) fn first_with<'a>(m: &'a Model, suffix: &str) -> Option<&'a Inst<'a>> {
    m.insts
        .iter()
        .find(|i| i.driver.as_deref().is_some_and(|d| d.ends_with(suffix)))
}

/// First thing to run on a board: load the design, read the FCLKs back from the
/// SLCR and compare with the HWH, then check that every interrupt has a UIO device.
pub fn board_check_example(m: &Model, d: &Design, crate_name: &str) -> String {
    let lib = crate_name.replace('-', "_");
    let mut s = String::from(HEADER);
    s.push_str("//! First on-board check (docs/08-verification-gaps.md: G1, G2, G4, G11): loads\n//! the bitstream, reads each FCLK back from the SLCR, and opens every interrupt's\n//! UIO device. Touches no IP register. Run as root: `cargo run --example board_check`.\n\n");
    let ultrascale = d.is_zynq_ultrascale_plus();
    if ultrascale {
        s = s
            .replace(
                "(docs/08-verification-gaps.md: G1, G2, G4, G11)",
                "(Zynq UltraScale+)",
            )
            .replace("from the SLCR", "from the clock registers");
    }
    let mut check = format!(
        "use hardroxide_mmio::MmioBackend;\n\nfn main() {{\n    let mut failed = 0;\n    println!(\"1. loading bitstream and applying clocks ...\");\n    let design = match {lib}::Design::load() {{\n        Ok(d) => {{\n            println!(\"   ok\");\n            Some(d)\n        }}\n        Err(e) => {{\n            println!(\"   FAILED: {{e}}\");\n            failed += 1;\n            None\n        }}\n    }};\n    if design.is_some() {{\n        println!(\"2. FCLK readback (SLCR) vs. the design:\");\n        let cfg = {lib}::embedded_load_config();\n        match hardroxide_mmio::DevMem::new().map(hardroxide_fpga::slcr::SLCR_BASE, hardroxide_fpga::slcr::SLCR_SIZE) {{\n            Ok(slcr) => for (i, want) in cfg.fclks {{\n                match hardroxide_fpga::slcr::read_pl_clock(&slcr, cfg.ps_clk_hz, i) {{\n                    Ok(got) if got.abs_diff(want) * 100 <= want => println!(\"   FCLK{{i}}: {{got}} Hz (want {{want}}) ok\"),\n                    Ok(got) => {{\n                        println!(\"   FCLK{{i}}: {{got}} Hz (want {{want}}) MISMATCH\");\n                        failed += 1;\n                    }}\n                    Err(e) => {{\n                        println!(\"   FCLK{{i}}: {{e}}\");\n                        failed += 1;\n                    }}\n                }}\n            }},\n            Err(e) => {{\n                println!(\"   cannot map the SLCR: {{e}}\");\n                failed += 1;\n            }}\n        }}\n    }}"
    );
    if ultrascale {
        // Replace the Zynq-7000's SLCR read-back with this family's clock registers.
        let from = check
            .find("        println!(\"2. FCLK readback")
            .expect("clock section");
        check.truncate(from);
        check.push_str(&format!(
            "        println!(\"2. PL clock readback vs. the design:\");\n        let cfg = {lib}::embedded_load_config();\n        use hardroxide_fpga::zynqmp as fpga;\n        let backend = hardroxide_mmio::DevMem::new();\n        match (backend.map(fpga::CRL_APB_BASE, fpga::CRL_APB_SIZE), backend.map(fpga::CRF_APB_BASE, fpga::CRF_APB_SIZE)) {{\n            (Ok(crl), Ok(crf)) => for (i, want) in cfg.fclks {{\n                match fpga::read_pl_clock(&crl, &crf, cfg.ps_clk_hz, i) {{\n                    Ok(got) if got.abs_diff(want) * 100 <= want => println!(\"   pl_clk{{i}}: {{got}} Hz (want {{want}}) ok\"),\n                    Ok(got) => {{\n                        println!(\"   pl_clk{{i}}: {{got}} Hz (want {{want}}) MISMATCH\");\n                        failed += 1;\n                    }}\n                    Err(e) => {{\n                        println!(\"   pl_clk{{i}}: {{e}}\");\n                        failed += 1;\n                    }}\n                }}\n            }},\n            (Err(e), _) | (_, Err(e)) => {{\n                println!(\"   cannot map the clock registers: {{e}}\");\n                failed += 1;\n            }}\n        }}\n    }}"
        ));
    }
    let _ = writeln!(s, "{check}");
    let mut any = false;
    for i in &m.insts {
        if irq_lines(i, d).is_empty() {
            continue;
        }
        if !any {
            let _ = writeln!(
                s,
                "    println!(\"3. interrupt UIO devices (needs interrupts.dtso applied):\");\n    use {lib}::pac;"
            );
            any = true;
        }
        let _ = writeln!(
            s,
            "    for line in pac::{}::IRQS {{\n        if line.via_intc.is_some() {{\n            continue;\n        }}\n        match line.open() {{\n            Ok(_) => println!(\"   {{}}.{{}}: ok\", line.instance, line.port),\n            Err(e) => {{\n                println!(\"   {{}}.{{}}: {{e}}\", line.instance, line.port);\n                failed += 1;\n            }}\n        }}\n    }}",
            i.ident
        );
    }
    let _ = writeln!(
        s,
        "    if failed > 0 {{\n        println!(\"{{failed}} check(s) failed\");\n        std::process::exit(1);\n    }}\n    println!(\"all checks passed; next run the per-IP examples\");\n}}"
    );
    s
}

/// Pin-free memory check: write a pattern over the whole block RAM and read it back.
pub fn bram_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = first_with(m, "::AxiBram")?;
    let lib = crate_name.replace('-', "_");
    Some(format!(
        "{HEADER}//! Hardware test for `{n}`: writes an address-derived pattern over the whole\n//! block RAM and reads it back. Run as root on the board.\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    let d = {lib}::Design::load()?;\n{sel}    let words = ram.len() / 4;\n    for i in 0..words {{\n        ram.write_word(4 * i, (i as u32).wrapping_mul(0x9E37_79B9))?;\n    }}\n    let mut bad = 0;\n    for i in 0..words {{\n        if ram.read_word(4 * i)? != (i as u32).wrapping_mul(0x9E37_79B9) {{\n            bad += 1;\n        }}\n    }}\n    println!(\"{{}} bytes: {{bad}} mismatching words\", ram.len());\n    if bad > 0 {{\n        std::process::exit(1);\n    }}\n    Ok(())\n}}\n",
        n = i.inst.name,
        sel = select("ram", &all_with(m, "::AxiBram"), false)
    ))
}

/// Stream FIFO check: push a packet in and read it back (needs TX looped to RX).
pub fn fifo_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = first_with(m, "::AxiFifo")?;
    let lib = crate_name.replace('-', "_");
    Some(format!(
        "{HEADER}//! Hardware test for `{n}`: sends a packet and reads it back (the stream\n//! transmit port must be looped to the receive port, as in the test fixture).\n//! Run as root on the board.\n\nuse std::time::Duration;\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    let d = {lib}::Design::load()?;\n{sel}    fifo.reset()?;\n    let packet: Vec<u8> = (0..37u8).collect();\n    fifo.send(&packet)?;\n    match fifo.recv(Duration::from_secs(1))? {{\n        Some(p) if p.starts_with(&packet) && p[packet.len()..].iter().all(|&b| b == 0) => {{\n            println!(\"loopback ok: sent {{}} bytes, core reported {{}} (rounded up to whole words)\", packet.len(), p.len())\n        }}\n        Some(p) => {{\n            println!(\"MISMATCH: sent {{}} bytes, got {{}}: {{p:?}}\", packet.len(), p.len());\n            std::process::exit(1);\n        }}\n        None => {{\n            println!(\"no packet came back within 1 s\");\n            std::process::exit(1);\n        }}\n    }}\n    Ok(())\n}}\n",
        n = i.inst.name,
        sel = select("fifo", &all_with(m, "::AxiFifo"), false)
    ))
}

/// UART check: send a greeting and print whatever arrives for one second.
pub fn uart_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = first_with(m, "::AxiUart")?;
    let lib = crate_name.replace('-', "_");
    Some(format!(
        "{HEADER}//! Hardware test for `{n}`: sends a greeting and prints what is received in\n//! the next second (connect TX to RX for a loopback). Run as root on the board.\n\nuse std::time::Duration;\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    let d = {lib}::Design::load()?;\n{sel}    uart.init()?;\n    uart.send(b\"hello from hardroxide\\n\", Duration::from_secs(1))?;\n    let mut buf = [0u8; 64];\n    let n = uart.receive(&mut buf, Duration::from_secs(1))?;\n    println!(\"received {{n}} bytes: {{:?}}\", String::from_utf8_lossy(&buf[..n]));\n    println!(\"errors: {{:?}}\", uart.errors()?);\n    Ok(())\n}}\n",
        n = i.inst.name,
        sel = select("uart", &all_with(m, "::AxiUart"), false)
    ))
}

/// Pin-free timer check: a 100 ms delay measured against the system clock.
pub fn timer_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = first_with(m, "::AxiTimer")?;
    let lib = crate_name.replace('-', "_");
    Some(format!(
        "{HEADER}//! Hardware smoke test for `{n}`: a 100 ms hardware delay measured with\n//! the system clock (expect ~100 ms). Run as root on the board.\n\nuse std::time::Instant;\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    let d = {lib}::Design::load()?;\n{sel}    let ticks = t.ticks_for_ns(100_000_000);\n    let t0 = Instant::now();\n    t.delay_ticks(ticks)?;\n    println!(\"{{}} Hz timer: asked 100 ms, took {{:?}}\", t.clock_hz(), t0.elapsed());\n    Ok(())\n}}\n",
        n = i.inst.name,
        sel = select("t", &all_with(m, "::AxiTimer"), false)
    ))
}

/// Pin-free SPI check using the core's internal loopback bit.
pub fn spi_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = first_with(m, "::AxiSpi")?;
    let lib = crate_name.replace('-', "_");
    Some(format!(
        "{HEADER}//! Hardware smoke test for `{n}`: internal loopback (SPICR.LOOP), so no\n//! wiring is needed. Run as root on the board.\n\nuse embedded_hal::spi::{{MODE_0, SpiBus}};\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    let mut d = {lib}::Design::load()?;\n{sel}    spi.init(MODE_0, false, true)?;\n    let sent: Vec<u8> = (0..40u8).map(|i| i.wrapping_mul(13).wrapping_add(5)).collect();\n    let mut buf = sent.clone();\n    spi.transfer_in_place(&mut buf)?;\n    assert_eq!(buf, sent, \"loopback mismatch\");\n    println!(\"SPI loopback OK: {{}} bytes\", buf.len());\n    Ok(())\n}}\n",
        n = i.inst.name,
        sel = select("spi", &all_with(m, "::AxiSpi"), true)
    ))
}

/// I2C bus scan (needs a real bus with pull-ups).
pub fn iic_example(m: &Model, crate_name: &str) -> Option<String> {
    let i = first_with(m, "::AxiIic")?;
    let lib = crate_name.replace('-', "_");
    Some(format!(
        "{HEADER}//! Hardware smoke test for `{n}`: probes every 7-bit address with an empty\n//! write and lists the ones that acknowledge. Needs a pulled-up I2C bus.\n\nuse embedded_hal::i2c::I2c;\n\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    let mut d = {lib}::Design::load()?;\n{sel}    i2c.init()?;\n    for addr in 0x08..0x78u8 {{\n        if i2c.write(addr, &[]).is_ok() {{\n            println!(\"found device at {{addr:#04x}}\");\n        }}\n    }}\n    Ok(())\n}}\n",
        n = i.inst.name,
        sel = select("i2c", &all_with(m, "::AxiIic"), true)
    ))
}
