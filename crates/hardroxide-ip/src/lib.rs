//! Curated drivers for Xilinx IP, written against [`hardroxide_mmio::Mmio`].

pub mod dma;
pub mod gpio;
pub mod hls;
pub mod iic;
pub mod spi;
pub mod timer;
pub use hardroxide_mmio::{Mmio, MmioError};

/// Test helper: look up `(register offset, field bit offset)` in a fixture HWH.
#[cfg(test)]
pub(crate) fn hwh(fixture: &str, inst: &str, reg: &str, field: Option<&str>) -> (usize, u32) {
    let x = hardroxide_hwh::Xsa::open(format!(
        "{}/../../fixtures/{fixture}.xsa",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let i = x
        .design
        .instance(inst)
        .unwrap_or_else(|| panic!("no instance {inst}"));
    let r = i.address_blocks[0]
        .registers
        .iter()
        .find(|r| r.name == reg)
        .unwrap_or_else(|| panic!("no register {reg}"));
    let bit = field.map_or(0, |f| {
        r.fields
            .iter()
            .find(|x| x.name == f)
            .unwrap_or_else(|| panic!("{reg}.{f}"))
            .bit_offset
    });
    (r.offset as usize, bit)
}
