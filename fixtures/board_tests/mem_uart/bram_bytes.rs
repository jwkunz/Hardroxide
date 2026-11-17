//! Block RAM as a byte buffer. Offsets must be word aligned (a misaligned one
//! is an error), and a final partial word is read-modify-written so the bytes
//! next to it keep their value. Run as root on the board: `sudo ./bram_bytes`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = design_1::Design::load()?;
    let ram = &d.axi_bram_ctrl_0;
    println!("block RAM: {} bytes", ram.len());

    // Start from a known pattern (whole words).
    let words: Vec<u32> = (0..16).map(|i| 0x1111_1111 * (i % 15)).collect();
    ram.write_words(0, &words)?;

    // Overwrite 7 bytes at offset 4: one whole word and 3 bytes of the next.
    ram.write_bytes(4, b"HELLO!!")?;

    // Read 16 bytes back; the 4th byte after the edit is untouched.
    let mut got = [0u8; 16];
    ram.read_bytes(0, &mut got)?;
    println!("bytes 0..16: {got:02x?}");

    let mut expect = Vec::new();
    for w in &words {
        expect.extend_from_slice(&w.to_le_bytes());
    }
    expect[4..11].copy_from_slice(b"HELLO!!");
    assert_eq!(
        &got[..],
        &expect[..16],
        "read-modify-write corrupted a neighbouring byte"
    );
    println!("ok: only bytes 4..11 changed");

    // A misaligned offset is refused rather than silently rounded.
    println!("write at offset 5 -> {:?}", ram.write_bytes(5, b"x"));
    Ok(())
}
