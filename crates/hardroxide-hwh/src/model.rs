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

impl Design {
    pub fn instance(&self, name: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.name == name)
    }

    /// Address-map entries belonging to `instance`.
    pub fn ranges_of<'a>(&'a self, instance: &'a str) -> impl Iterator<Item = &'a MemRange> {
        self.memory_ranges.iter().filter(move |r| r.instance == instance)
    }
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptPort {
    pub name: String,
    /// Name of the signal net, used to find the sink.
    pub signal: String,
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
