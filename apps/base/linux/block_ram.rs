//! PYNQ-Z2 base overlay under Linux: memory inside the FPGA, as words and as bytes.
//!
//! Uses the 64 KiB of block RAM behind `iop_arduino_mb_bram_ctrl`. It is the program memory of the overlay's Arduino
//! MicroBlaze; nothing here starts that processor, and a program that does must not share the memory like this. The
//! program fills the whole of it and reads it back, then edits a few bytes. It overwrites what it tests. Run as root.
//! Linux tutorial 3.
use design_1::Design;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let d = Design::load()?;
    let ram = &d.iop_arduino_mb_bram_ctrl;
    println!("block RAM: {} bytes", ram.len());

    // A different value in every word, written and read back in one call each.
    let words: Vec<u32> = (0..ram.len() as u32 / 4)
        .map(|i| i.wrapping_mul(0x9E37_79B9))
        .collect();
    ram.write_words(0, &words)?;
    let mut back = vec![0u32; words.len()];
    ram.read_words(0, &mut back)?;
    let wrong = words.iter().zip(&back).filter(|(a, b)| a != b).count();
    println!("{} words written and read back: {wrong} wrong", words.len());
    if wrong != 0 {
        return Err("the memory does not hold what was written".into());
    }

    // The same memory as bytes. Overwrite 7 bytes at offset 4: one whole word and 3 bytes of the next.
    ram.write_bytes(4, b"HELLO!!")?;
    let mut got = [0u8; 16];
    ram.read_bytes(0, &mut got)?;
    println!("bytes 0..16: {got:02x?}");
    let mut expect: Vec<u8> = words[..4].iter().flat_map(|w| w.to_le_bytes()).collect();
    expect[4..11].copy_from_slice(b"HELLO!!");
    if got[..] != expect[..] {
        return Err("a byte next to the edit changed".into());
    }
    println!("only bytes 4..11 changed");

    // An offset between two words is refused, not rounded.
    println!("write at offset 5 -> {:?}", ram.write_bytes(5, b"x"));
    println!("BLOCK RAM OK");
    Ok(())
}
