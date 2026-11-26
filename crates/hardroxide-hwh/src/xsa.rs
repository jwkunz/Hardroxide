use crate::{Design, Error, parse_hwh};
use std::io::{Read, Seek};
use std::path::Path;

/// An opened XSA: the parsed design plus the raw `.bit` bytes.
#[derive(Debug, Clone)]
pub struct Xsa {
    pub design: Design,
    /// Raw contents of the `.bit` file (with Xilinx header).
    pub bit: Vec<u8>,
}

impl Xsa {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_reader(std::fs::File::open(path)?)
    }

    pub fn from_reader(r: impl Read + Seek) -> Result<Self, Error> {
        let mut zip = zip::ZipArchive::new(r)?;
        let hwh_name = design_hwh(&mut zip)?;
        let bit_name = unique(&zip, "bit")?;
        let mut xml = String::new();
        zip.by_name(&hwh_name)?.read_to_string(&mut xml)?;
        let mut bit = Vec::new();
        zip.by_name(&bit_name)?.read_to_end(&mut bit)?;
        Ok(Xsa {
            design: parse_hwh(&xml)?,
            bit,
        })
    }
}

impl Xsa {
    /// The raw `.bin` for `fpga_manager` (see `BitFile::to_bin_swapped`).
    pub fn bin(&self) -> Result<Vec<u8>, Error> {
        Ok(crate::BitFile::parse(&self.bit)?.to_bin_swapped()?)
    }
}

/// The block design's handoff file. An XSA can hold more (a Zynq UltraScale+ design's SmartConnects each bring their
/// own); the design's is then the one with the processing system in it.
fn design_hwh<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<String, Error> {
    let several = match unique(zip, "hwh") {
        Err(Error::AmbiguousEntry(_)) => names(zip, "hwh"),
        one => return one,
    };
    let mut with_ps = Vec::new();
    for name in several {
        let mut xml = String::new();
        zip.by_name(&name)?.read_to_string(&mut xml)?;
        if ["processing_system7", "zynq_ultra_ps_e"]
            .iter()
            .any(|ps| xml.contains(&format!("MODTYPE=\"{ps}\"")))
        {
            with_ps.push(name);
        }
    }
    match <[String; 1]>::try_from(with_ps) {
        Ok([name]) => Ok(name),
        Err(_) => Err(Error::AmbiguousEntry("hwh")),
    }
}

/// The entries with extension `ext`, sorted.
fn names<R: Read + Seek>(zip: &zip::ZipArchive<R>, ext: &str) -> Vec<String> {
    let mut names: Vec<_> = zip
        .file_names()
        .filter(|n| {
            n.rsplit_once('.')
                .is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
        })
        .map(String::from)
        .collect();
    names.sort();
    names
}

fn unique<R: Read + Seek>(zip: &zip::ZipArchive<R>, ext: &'static str) -> Result<String, Error> {
    let mut names = names(zip, ext);
    match names.len() {
        0 => Err(Error::MissingEntry(ext)),
        1 => Ok(names.remove(0)),
        _ => Err(Error::AmbiguousEntry(ext)),
    }
}
