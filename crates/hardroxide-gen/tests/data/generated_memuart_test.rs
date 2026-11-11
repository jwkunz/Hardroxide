use hardroxide_mmio::MockBackend;
use pynq_z2_memuart_test::{Design, pac};
use std::time::Duration;

#[test]
fn bram_is_a_bounds_checked_memory() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    assert_eq!(d.axi_bram_ctrl_0.len(), pac::axi_bram_ctrl_0::SIZE);
    let base = pac::axi_bram_ctrl_0::BASE_ADDRESS;
    d.axi_bram_ctrl_0.write_bytes(0x10, b"abcdef").unwrap();
    assert_eq!(be.peek(base + 0x10), u32::from_le_bytes(*b"abcd"));
    assert!(d.axi_bram_ctrl_0.read_word(d.axi_bram_ctrl_0.len()).is_err());
}

#[test]
fn uart_writes_the_tx_fifo() {
    let be = MockBackend::new();
    let d = Design::map_only(be.clone()).unwrap();
    let base = pac::axi_uartlite_0::BASE_ADDRESS;
    d.axi_uartlite_0.init().unwrap();
    assert_eq!(be.peek(base + 0x0C), 0b11); // reset both FIFOs
    d.axi_uartlite_0.send(b"A", Duration::from_millis(10)).unwrap();
    assert_eq!(be.peek(base + 0x04), b'A' as u32);
    be.poke(base + 0x08, 1); // STAT: RX valid
    be.poke(base, b'z' as u32);
    assert_eq!(d.axi_uartlite_0.try_read_byte().unwrap(), Some(b'z'));
}
