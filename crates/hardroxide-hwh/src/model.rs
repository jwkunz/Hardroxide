use std::collections::BTreeMap;

/// A parsed hardware design (the contents of an HWH file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Design {
    /// Block design name (`SYSTEMINFO NAME`).
    pub name: String,
    pub vivado_version: String,
    pub arch: String,
    pub device: String,
    pub package: String,
    pub speed_grade: String,
    /// All module instances, in document order.
    pub instances: Vec<Instance>,
    /// Address-map entries (slave windows as seen by a master).
    pub memory_ranges: Vec<MemRange>,
    /// PL clocks (FCLK_CLK0..3) derived from the Zynq-7000 PS7 instance.
    pub pl_clocks: Vec<PlClock>,
    /// PS reference clock in Hz (`PCW_CRYSTAL_PERIPHERAL_FREQMHZ`); 0 if absent.
    pub ps_clk_hz: u64,
}

/// Zynq-7000 GIC SPI *device-tree cell* for PS7 `IRQ_F2P[bit]` (UG585 table
/// 7-4: `IRQ_F2P[7:0]` are interrupt IDs 61..=68 and `[15:8]` are 84..=91; the
/// device-tree `interrupts = <0 N 4>` cell is ID - 32). `None` for bits >= 16.
/// UNVERIFIED on hardware (see docs, G11).
pub fn zynq7000_f2p_spi_cell(bit: u32) -> Option<u32> {
    match bit {
        0..=7 => Some(29 + bit),
        8..=15 => Some(52 + (bit - 8)),
        _ => None,
    }
}

impl Instance {
    /// Frequency of the AXI-Lite clock (`s_axi_aclk`, else the first clock
    /// port that reports one), in Hz.
    pub fn axi_clk_hz(&self) -> Option<u64> {
        let by = |p: &&Port| p.clk_hz.is_some();
        self.ports
            .iter()
            .filter(by)
            .find(|p| p.name.eq_ignore_ascii_case("s_axi_aclk"))
            .or_else(|| self.ports.iter().find(by))
            .and_then(|p| p.clk_hz)
    }

    /// Values the generated driver constructors are built from (beyond the
    /// register map): DMA length-register width, SPI FIFO depth and
    /// slave-select count, the Timer's AXI clock. They are part of the
    /// interface hash, so a bitstream-only `hardroxide update` cannot leave a
    /// stale value compiled into `lib.rs`. The generator reads them only
    /// through this method.
    pub fn driver_config(&self) -> BTreeMap<&'static str, String> {
        let mut m = BTreeMap::new();
        let mut take = |k: &'static str| {
            if let Some(v) = self.parameters.get(k) {
                m.insert(k, v.clone());
            }
        };
        match self.mod_type.as_str() {
            "axi_dma" => take("C_SG_LENGTH_WIDTH"),
            "axi_quad_spi" => {
                take("C_FIFO_EXIST");
                take("C_FIFO_DEPTH");
                take("C_NUM_SS_BITS");
            }
            "axi_timer" => {
                if let Some(hz) = self.axi_clk_hz() {
                    m.insert("axi_clk_hz", hz.to_string());
                }
            }
            _ => {}
        }
        m
    }

    /// Vitis HLS-generated IP (`vendor:hls:name:version`).
    pub fn is_hls(&self) -> bool {
        self.vlnv.split(':').nth(1) == Some("hls")
    }
}

impl Design {
    /// `(FCLK index, Hz)` for each enabled PL clock.
    pub fn enabled_fclks(&self) -> Vec<(u8, u64)> {
        self.pl_clocks
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.index, c.freq_hz))
            .collect()
    }

    /// Bit of the PS7 `IRQ_F2P` vector driven by output port `port` of
    /// `instance`, following `xlconcat` blocks. `None` if the port is wider
    /// than one bit, unconnected, fans out, goes through other logic, or
    /// ends at an AXI interrupt controller (see [`Design::irq_route`]).
    pub fn ps7_irq_f2p_bit(&self, instance: &str, port: &str) -> Option<u32> {
        match self.irq_route(instance, port)? {
            IrqRoute::Ps7 { bit } => Some(bit),
            IrqRoute::Intc { .. } => None,
        }
    }

    /// Where the one-bit output `port` of `instance` ends up: a bit of the
    /// PS7 `IRQ_F2P` vector or an input of an `axi_intc`, following
    /// `xlconcat` blocks. `None` if wider than one bit, unconnected, fanned
    /// out, or routed through other logic.
    pub fn irq_route(&self, instance: &str, port: &str) -> Option<IrqRoute> {
        let p = self
            .instance(instance)?
            .ports
            .iter()
            .find(|p| p.name == port)?;
        if p.width != 1 {
            return None;
        }
        self.trace_irq(instance, port, 0)
    }

    fn trace_irq(&self, instance: &str, port: &str, depth: u32) -> Option<IrqRoute> {
        if depth > 8 {
            return None;
        }
        let p = self
            .instance(instance)?
            .ports
            .iter()
            .find(|p| p.name == port)?;
        let [(sink_inst, sink_port)] = p.sinks.as_slice() else {
            return None;
        };
        let sink = self.instance(sink_inst)?;
        match sink.mod_type.as_str() {
            "processing_system7" if sink_port == "IRQ_F2P" => Some(IrqRoute::Ps7 { bit: 0 }),
            "axi_intc" if sink_port == "intr" => Some(IrqRoute::Intc {
                controller: sink_inst.clone(),
                bit: 0,
            }),
            "xlconcat" => {
                let k: u32 = sink_port.strip_prefix("In")?.parse().ok()?;
                let below: u32 = (0..k)
                    .map(|j| {
                        sink.parameters
                            .get(&format!("IN{j}_WIDTH"))
                            .and_then(|v| v.parse::<u32>().ok())
                    })
                    .sum::<Option<u32>>()?;
                Some(match self.trace_irq(sink_inst, "dout", depth + 1)? {
                    IrqRoute::Ps7 { bit } => IrqRoute::Ps7 { bit: below + bit },
                    IrqRoute::Intc { controller, bit } => IrqRoute::Intc {
                        controller,
                        bit: below + bit,
                    },
                })
            }
            _ => None,
        }
    }

    pub fn instance(&self, name: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.name == name)
    }

    /// Address-map entries belonging to `instance`.
    pub fn ranges_of<'a>(&'a self, instance: &'a str) -> impl Iterator<Item = &'a MemRange> {
        self.memory_ranges
            .iter()
            .filter(move |r| r.instance == instance)
    }
}

/// Destination of a PL interrupt signal (see [`Design::irq_route`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrqRoute {
    /// Bit of the PS7 `IRQ_F2P` vector.
    Ps7 { bit: u32 },
    /// Input `bit` of the AXI interrupt controller `controller`.
    Intc { controller: String, bit: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// Vivado instance name, e.g. `axi_gpio_0`.
    pub name: String,
    /// `vendor:library:name:version`, e.g. `xilinx.com:ip:axi_gpio:2.0`.
    pub vlnv: String,
    /// `MODTYPE`, e.g. `axi_gpio`.
    pub mod_type: String,
    /// `IS_PL="FALSE"` marks the processing system.
    pub is_ps: bool,
    pub parameters: BTreeMap<String, String>,
    pub address_blocks: Vec<AddressBlock>,
    /// Interrupt output ports (`SIGIS="INTERRUPT"`, direction out).
    pub interrupts: Vec<InterruptPort>,
    /// All ports with their connections (used for net tracing).
    pub ports: Vec<Port>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    /// `I`, `O` or `IO`.
    pub dir: String,
    pub width: u32,
    pub sinks: Vec<(String, String)>,
    /// `CLKFREQUENCY` of clock ports, in Hz.
    pub clk_hz: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptPort {
    pub name: String,
    /// Name of the signal net.
    pub signal: String,
    /// Connected `(instance, port)` sinks, in document order. Empty if the
    /// interrupt output is unconnected.
    pub sinks: Vec<(String, String)>,
    /// Port width in bits.
    pub width: u32,
    /// `SENSITIVITY` attribute, e.g. `LEVEL_HIGH` or `EDGE_RISING`
    /// (empty if absent).
    pub sensitivity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressBlock {
    pub name: String,
    /// Bus interface the block is reached through (`S_AXI`).
    pub interface: String,
    /// Size in bytes.
    pub range: u64,
    pub usage: String,
    pub registers: Vec<Register>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Register {
    pub name: String,
    pub description: String,
    /// Byte offset within the address block.
    pub offset: u64,
    /// Raw `SIZE` property. Vivado is inconsistent here (AXI GPIO data
    /// registers report their *field* width), so treat as a hint only.
    pub size: u32,
    pub access: Access,
    pub reset_value: u64,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub description: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    ReadWrite,
    ReadOnly,
    WriteOnly,
    /// Anything else (e.g. `read-writeOnce`); unknown strings land here.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemRange {
    /// Slave instance.
    pub instance: String,
    pub address_block: String,
    pub slave_interface: String,
    pub master_interface: String,
    pub base: u64,
    /// Inclusive high address.
    pub high: u64,
}

impl MemRange {
    pub fn size(&self) -> u64 {
        self.high - self.base + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlClock {
    /// FCLK index, 0..=3.
    pub index: u8,
    pub enabled: bool,
    /// Actual frequency in Hz (PS7 `PCW_ACT_FPGAn_PERIPHERAL_FREQMHZ`).
    pub freq_hz: u64,
    /// Matching `FCLK_RESETn_N` enabled.
    pub reset_enabled: bool,
}
