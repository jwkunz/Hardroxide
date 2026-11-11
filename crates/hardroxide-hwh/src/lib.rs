//! Typed model of a Vivado hardware handoff (`.hwh`) and XSA archive.
//!
//! Everything here is deterministic: collections are ordered `Vec`s in
//! document order or `BTreeMap`s.

mod bit;
mod model;
mod parse;
mod xsa;

pub use bit::{BitError, BitFile};
pub use model::*;
pub use parse::parse_hwh;
pub use xsa::Xsa;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a valid XSA archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("XSA contains no `.{0}` file")]
    MissingEntry(&'static str),
    #[error("XSA contains more than one `.{0}` file")]
    AmbiguousEntry(&'static str),
    #[error("malformed HWH XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("malformed HWH: {0}")]
    Hwh(String),
    #[error("bitstream error: {0}")]
    Bit(#[from] BitError),
}
