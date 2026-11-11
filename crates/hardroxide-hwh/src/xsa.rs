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
        let hwh_name = unique(&zip, "hwh")?;
        let bit_name = unique(&zip, "bit")?;
        let mut xml = String::new();
        zip.by_name(&hwh_name)?.read_to_string(&mut xml)?;
        let mut bit = Vec::new();
        zip.by_name(&bit_name)?.read_to_end(&mut bit)?;
        Ok(Xsa { design: parse_hwh(&xml)?, bit })
    }
}

fn unique<R: Read + Seek>(zip: &zip::ZipArchive<R>, ext: &'static str) -> Result<String, Error> {
    let mut names: Vec<_> = zip
        .file_names()
        .filter(|n| n.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext)))
        .map(String::from)
        .collect();
    names.sort();
    match names.len() {
        0 => Err(Error::MissingEntry(ext)),
        1 => Ok(names.remove(0)),
        _ => Err(Error::AmbiguousEntry(ext)),
    }
}
