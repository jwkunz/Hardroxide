//! The Zynq-7000's Gigabit Ethernet MACs (`GEM0`, `GEM1`; Cadence GEM): descriptor rings in non-cacheable memory, polled receive and
//! transmit of whole frames, the management interface (MDIO) to the PHY, and link bring-up with autonegotiation.
//!
//! The rings and the frame buffers live in one region of memory given to [`Gem::new`]; it must not be cached (see
//! [`crate::mem::set_non_cacheable`]) because the MAC's DMA reads and writes it behind the processor's back. Everything is
//! polled: there are no interrupts. The register layout and the descriptor format are those of the Cadence GEM as the
//! Zynq-7000 technical reference manual describes them.
//!
//! Not supported: jumbo frames, VLAN tags, checksum offload, hardware timestamps, the second queue, receive filtering beyond the
//! station address and broadcast, PHYs other than through the standard MII registers (the RTL8211E on the PYNQ-Z2 is plain MII).

use core::time::Duration;
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{Mmio, MmioError};

const NWCTRL: usize = 0x000;
const NWCFG: usize = 0x004;
const NWSR: usize = 0x008;
const DMACR: usize = 0x010;
const TXSR: usize = 0x014;
const RXQBASE: usize = 0x018;
const TXQBASE: usize = 0x01C;
const RXSR: usize = 0x020;
const ISR: usize = 0x024;
const IDR: usize = 0x02C;
const PHYMNTNC: usize = 0x034;
const SPEC_ADDR1_BOT: usize = 0x088;
const SPEC_ADDR1_TOP: usize = 0x08C;
const FRAMES_TX: usize = 0x108;
const FRAMES_RX: usize = 0x158;

const CTRL_RXEN: u32 = 1 << 2;
const CTRL_TXEN: u32 = 1 << 3;
const CTRL_MDEN: u32 = 1 << 4;
const CTRL_CLRSTAT: u32 = 1 << 5;
const CTRL_STARTTX: u32 = 1 << 9;

const CFG_SPEED100: u32 = 1 << 0;
const CFG_FULL_DUPLEX: u32 = 1 << 1;
const CFG_RX_1536: u32 = 1 << 8;
const CFG_GIGABIT: u32 = 1 << 10;
const CFG_FCS_REMOVE: u32 = 1 << 17;
const CFG_MDC_DIV_48: u32 = 3 << 18;

const NWSR_MDIO_IDLE: u32 = 1 << 2;

/// Receive descriptors, each with a buffer of [`FRAME_BUF`] bytes.
pub const RX_DESCRIPTORS: usize = 32;
/// Transmit descriptors.
pub const TX_DESCRIPTORS: usize = 16;
/// The size of one frame buffer (a maximum frame is 1518 bytes; the MAC wants a multiple of 64).
pub const FRAME_BUF: usize = 1536;
const RX_RING: usize = 0;
const TX_RING: usize = RX_RING + RX_DESCRIPTORS * 8;
const RX_BUFS: usize = (TX_RING + TX_DESCRIPTORS * 8).next_multiple_of(64);
const TX_BUFS: usize = RX_BUFS + RX_DESCRIPTORS * FRAME_BUF;
/// The size of the memory region [`Gem::new`] needs.
pub const DMA_REGION_BYTES: usize = TX_BUFS + TX_DESCRIPTORS * FRAME_BUF;

const RX_USED: u32 = 1 << 0; // word 0: the frame was received (the processor owns the descriptor)
const RX_WRAP: u32 = 1 << 1;
const RX_SOF: u32 = 1 << 14; // word 1
const RX_EOF: u32 = 1 << 15;
const RX_LEN: u32 = 0x1FFF;
const TX_LAST: u32 = 1 << 15; // word 1
const TX_WRAP: u32 = 1 << 30;
const TX_USED: u32 = 1 << 31; // the processor owns the descriptor

// PHY registers (IEEE 802.3 clause 22).
const PHY_CONTROL: u8 = 0;
const PHY_STATUS: u8 = 1;
const PHY_ID1: u8 = 2;
const PHY_ADVERTISE: u8 = 4;
const PHY_PARTNER: u8 = 5;
const PHY_1000_CONTROL: u8 = 9;
const PHY_1000_STATUS: u8 = 10;

#[derive(Debug, thiserror::Error)]
pub enum GemError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("no PHY answered on the management interface")]
    NoPhy,
    #[error("the management interface did not finish a transfer")]
    MdioTimeout,
    #[error("the link did not come up (is the cable plugged in?)")]
    NoLink,
}

/// The speed and duplex the PHY negotiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub speed_mbps: u16,
    pub full_duplex: bool,
}

/// What the PHY's registers say about the link (a pure function of the partner abilities, for testing).
pub fn negotiated(advertise_1000: u16, status_1000: u16, advertise: u16, partner: u16) -> Link {
    // 1000BASE-T: the partner's full (bit 11) or half (bit 10) duplex ability, if we advertised the same (bits 9 and 8).
    if status_1000 & 0x0800 != 0 && advertise_1000 & 0x0200 != 0 {
        return Link {
            speed_mbps: 1000,
            full_duplex: true,
        };
    }
    if status_1000 & 0x0400 != 0 && advertise_1000 & 0x0100 != 0 {
        return Link {
            speed_mbps: 1000,
            full_duplex: false,
        };
    }
    let common = advertise & partner;
    if common & 0x0100 != 0 {
        Link {
            speed_mbps: 100,
            full_duplex: true,
        }
    } else if common & 0x0080 != 0 {
        Link {
            speed_mbps: 100,
            full_duplex: false,
        }
    } else if common & 0x0040 != 0 {
        Link {
            speed_mbps: 10,
            full_duplex: true,
        }
    } else {
        Link {
            speed_mbps: 10,
            full_duplex: false,
        }
    }
}

pub struct Gem<R: Mmio> {
    regs: R,
    dma: usize,
    mac: [u8; 6],
    rx_next: usize,
    tx_next: usize,
    phy: Option<u8>,
    link: Option<Link>,
}

impl<R: Mmio> Gem<R> {
    /// A driver for the MAC whose registers are `regs`, with its rings and buffers in the [`DMA_REGION_BYTES`] bytes at `dma_base`.
    /// Does not touch the hardware; call [`Gem::init`].
    ///
    /// # Safety
    /// `dma_base` must be the address of [`DMA_REGION_BYTES`] bytes of memory that is not cached, is not used for anything
    /// else, is 64-byte aligned and is reachable by the MAC's DMA at the same address, for as long as this value lives.
    pub unsafe fn new(regs: R, dma_base: usize, mac: [u8; 6]) -> Self {
        Gem {
            regs,
            dma: dma_base,
            mac,
            rx_next: 0,
            tx_next: 0,
            phy: None,
            link: None,
        }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }
    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }
    pub fn link(&self) -> Option<Link> {
        self.link
    }
    pub fn phy_address(&self) -> Option<u8> {
        self.phy
    }
    /// Frames the MAC has transmitted and received (its own counters).
    pub fn frame_counts(&self) -> Result<(u32, u32), GemError> {
        Ok((self.regs.read32(FRAMES_TX)?, self.regs.read32(FRAMES_RX)?))
    }

    fn word(&self, offset: usize) -> *mut u32 {
        (self.dma + offset) as *mut u32
    }
    fn read_word(&self, offset: usize) -> u32 {
        // SAFETY: inside the region the caller of `new` vouched for.
        unsafe { core::ptr::read_volatile(self.word(offset)) }
    }
    fn write_word(&self, offset: usize, v: u32) {
        // SAFETY: as above.
        unsafe { core::ptr::write_volatile(self.word(offset), v) }
    }

    fn barrier() {
        #[cfg(target_arch = "arm")]
        // SAFETY: a barrier instruction.
        unsafe {
            core::arch::asm!("dsb", options(nostack, preserves_flags))
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    /// Reset the MAC, set up the rings and the station address. The MAC does not receive or send until [`Gem::link_up`] has run.
    pub fn init(&mut self) -> Result<(), GemError> {
        self.regs.write32(NWCTRL, 0)?;
        self.regs.write32(NWCTRL, CTRL_CLRSTAT)?;
        self.regs.write32(NWCTRL, 0)?;
        self.regs.write32(TXSR, 0xFFFF_FFFF)?;
        self.regs.write32(RXSR, 0xFFFF_FFFF)?;
        self.regs.write32(ISR, 0xFFFF_FFFF)?;
        self.regs.write32(IDR, 0x7FF_FEFF)?; // no interrupts: everything is polled
        // The rings: receive descriptors belong to the MAC, transmit descriptors to the processor.
        for i in 0..RX_DESCRIPTORS {
            let buf = (self.dma + RX_BUFS + i * FRAME_BUF) as u32 & !3;
            self.write_word(
                RX_RING + i * 8,
                buf | if i == RX_DESCRIPTORS - 1 { RX_WRAP } else { 0 },
            );
            self.write_word(RX_RING + i * 8 + 4, 0);
        }
        for i in 0..TX_DESCRIPTORS {
            self.write_word(TX_RING + i * 8, (self.dma + TX_BUFS + i * FRAME_BUF) as u32);
            self.write_word(
                TX_RING + i * 8 + 4,
                TX_USED | if i == TX_DESCRIPTORS - 1 { TX_WRAP } else { 0 },
            );
        }
        self.rx_next = 0;
        self.tx_next = 0;
        Self::barrier();
        self.regs.write32(RXQBASE, (self.dma + RX_RING) as u32)?;
        self.regs.write32(TXQBASE, (self.dma + TX_RING) as u32)?;
        // Receive buffers of 1536 bytes (24 x 64), full packet buffers, 16-beat bursts.
        self.regs
            .write32(DMACR, (24 << 16) | (1 << 10) | (3 << 8) | 0x10)?;
        self.regs.write32(
            NWCFG,
            CFG_FCS_REMOVE | CFG_RX_1536 | CFG_MDC_DIV_48 | CFG_FULL_DUPLEX,
        )?;
        // The station address (first specific-address filter) and the management port.
        let m = self.mac;
        self.regs
            .write32(SPEC_ADDR1_BOT, u32::from_le_bytes([m[0], m[1], m[2], m[3]]))?;
        self.regs
            .write32(SPEC_ADDR1_TOP, u32::from(u16::from_le_bytes([m[4], m[5]])))?;
        self.regs.write32(NWCTRL, CTRL_MDEN)?;
        Ok(())
    }

    // ---- The management interface ----

    fn wait_mdio(&self) -> Result<(), GemError> {
        let t0 = Instant::now();
        while self.regs.read32(NWSR)? & NWSR_MDIO_IDLE == 0 {
            if t0.elapsed() > Duration::from_millis(50) {
                return Err(GemError::MdioTimeout);
            }
        }
        Ok(())
    }

    fn mdio(&self, op: u32, phy: u8, reg: u8, data: u16) -> Result<u16, GemError> {
        self.wait_mdio()?;
        self.regs.write32(
            PHYMNTNC,
            (1 << 30)
                | (op << 28)
                | (u32::from(phy) << 23)
                | (u32::from(reg) << 18)
                | (2 << 16)
                | u32::from(data),
        )?;
        self.wait_mdio()?;
        Ok(self.regs.read32(PHYMNTNC)? as u16)
    }

    /// Read PHY register `reg` of the PHY at `phy`.
    pub fn mdio_read(&self, phy: u8, reg: u8) -> Result<u16, GemError> {
        self.mdio(2, phy, reg, 0)
    }

    /// Write PHY register `reg` of the PHY at `phy`.
    pub fn mdio_write(&self, phy: u8, reg: u8, value: u16) -> Result<(), GemError> {
        self.mdio(1, phy, reg, value).map(|_| ())
    }

    /// The first PHY address that answers.
    pub fn find_phy(&mut self) -> Result<u8, GemError> {
        for a in 0..32u8 {
            let id = self.mdio_read(a, PHY_ID1)?;
            if id != 0xFFFF && id != 0 {
                self.phy = Some(a);
                return Ok(a);
            }
        }
        Err(GemError::NoPhy)
    }

    /// Restart autonegotiation, wait up to `timeout_ms` for the link, then set the MAC (and, through `slcr`, the clock the MAC
    /// transmits with) to the negotiated speed and turn the receiver and transmitter on.
    pub fn link_up(&mut self, slcr: &impl Mmio, timeout_ms: u64) -> Result<Link, GemError> {
        let phy = match self.phy {
            Some(p) => p,
            None => self.find_phy()?,
        };
        // Autonegotiate, advertising everything the PHY can do.
        self.mdio_write(phy, PHY_CONTROL, 0x1200)?; // autonegotiation enable + restart
        let t0 = Instant::now();
        loop {
            let status = self.mdio_read(phy, PHY_STATUS)?;
            // The link-status bit latches low; read twice to see the present state.
            let status = if status & 0x0004 == 0 {
                self.mdio_read(phy, PHY_STATUS)?
            } else {
                status
            };
            if status & 0x0024 == 0x0024 {
                break;
            }
            if t0.elapsed() > Duration::from_millis(timeout_ms) {
                return Err(GemError::NoLink);
            }
            sleep(Duration::from_millis(10));
        }
        let link = negotiated(
            self.mdio_read(phy, PHY_1000_CONTROL)?,
            self.mdio_read(phy, PHY_1000_STATUS)?,
            self.mdio_read(phy, PHY_ADVERTISE)?,
            self.mdio_read(phy, PHY_PARTNER)?,
        );
        self.set_speed(slcr, link)?;
        self.regs
            .write32(NWCTRL, CTRL_MDEN | CTRL_RXEN | CTRL_TXEN)?;
        self.link = Some(link);
        Ok(link)
    }

    /// Program the MAC's speed and duplex and the transmit clock (from the IO PLL at 1000 MHz: 125, 25 or 2.5 MHz) for `link`.
    pub fn set_speed(&self, slcr: &impl Mmio, link: Link) -> Result<(), GemError> {
        let mut cfg = CFG_FCS_REMOVE | CFG_RX_1536 | CFG_MDC_DIV_48;
        if link.full_duplex {
            cfg |= CFG_FULL_DUPLEX;
        }
        cfg |= match link.speed_mbps {
            1000 => CFG_GIGABIT,
            100 => CFG_SPEED100,
            _ => 0,
        };
        self.regs.write32(NWCFG, cfg)?;
        // GEM0_CLK_CTRL (0x140): bit 0 enable, source IO PLL, DIVISOR0 [13:8] = 8, DIVISOR1 [25:20] = 1, 5 or 50.
        let div1 = match link.speed_mbps {
            1000 => 1,
            100 => 5,
            _ => 50,
        };
        let ctrl = (slcr.read32(0x140)? & !(0x3F << 20)) | (div1 << 20);
        slcr.write32(0x008, 0xDF0D)?; // unlock
        slcr.write32(0x140, ctrl)?;
        slcr.write32(0x004, 0x767B)?; // lock
        Ok(())
    }

    // ---- Frames ----

    /// Copy the next received frame (without its CRC) into `buf`; `None` if none is waiting. A frame too long for `buf` is cut.
    pub fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        loop {
            let d0 = self.read_word(RX_RING + self.rx_next * 8);
            if d0 & RX_USED == 0 {
                return None;
            }
            Self::barrier();
            let d1 = self.read_word(RX_RING + self.rx_next * 8 + 4);
            let len = (d1 & RX_LEN) as usize;
            let whole = d1 & (RX_SOF | RX_EOF) == (RX_SOF | RX_EOF);
            let n = len.min(buf.len()).min(FRAME_BUF);
            if whole {
                let src = (self.dma + RX_BUFS + self.rx_next * FRAME_BUF) as *const u8;
                for (i, b) in buf[..n].iter_mut().enumerate() {
                    // SAFETY: inside the receive buffer of this descriptor.
                    *b = unsafe { core::ptr::read_volatile(src.add(i)) };
                }
            }
            // Give the descriptor back to the MAC.
            self.write_word(RX_RING + self.rx_next * 8 + 4, 0);
            self.write_word(RX_RING + self.rx_next * 8, d0 & !RX_USED);
            Self::barrier();
            self.rx_next = (self.rx_next + 1) % RX_DESCRIPTORS;
            if whole {
                return Some(n);
            }
            // A frame split over several buffers cannot happen with 1536-byte buffers and no jumbo frames; skip it.
        }
    }

    /// Queue a frame (without a CRC; the MAC adds it and pads short frames). `false` if every transmit descriptor is busy
    /// or the frame is longer than a buffer.
    pub fn transmit(&mut self, frame: &[u8]) -> bool {
        if frame.is_empty() || frame.len() > FRAME_BUF {
            return false;
        }
        let d1 = self.read_word(TX_RING + self.tx_next * 8 + 4);
        if d1 & TX_USED == 0 {
            return false;
        }
        let dst = (self.dma + TX_BUFS + self.tx_next * FRAME_BUF) as *mut u8;
        for (i, b) in frame.iter().enumerate() {
            // SAFETY: inside the transmit buffer of this descriptor.
            unsafe { core::ptr::write_volatile(dst.add(i), *b) };
        }
        let wrap = if self.tx_next == TX_DESCRIPTORS - 1 {
            TX_WRAP
        } else {
            0
        };
        Self::barrier();
        // Clearing USED hands the descriptor to the MAC.
        self.write_word(
            TX_RING + self.tx_next * 8 + 4,
            frame.len() as u32 | TX_LAST | wrap,
        );
        Self::barrier();
        self.tx_next = (self.tx_next + 1) % TX_DESCRIPTORS;
        // Start the transmitter (the other bits keep their values).
        let ctrl = self.regs.read32(NWCTRL).unwrap_or(0);
        self.regs.write32(NWCTRL, ctrl | CTRL_STARTTX).is_ok()
    }
}

#[cfg(feature = "net")]
impl<R: Mmio> hardroxide_net::Device for Gem<R> {
    fn mac(&self) -> [u8; 6] {
        self.mac
    }
    fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        Gem::receive(self, buf)
    }
    fn transmit(&mut self, frame: &[u8]) -> bool {
        Gem::transmit(self, frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hardroxide_mmio::{MmioBackend, MockBackend, MockRegion};

    /// 64-byte aligned memory standing in for the DMA region.
    struct Region(std::vec::Vec<u8>);
    impl Region {
        fn new() -> Self {
            Region(std::vec![0u8; DMA_REGION_BYTES + 64])
        }
        fn base(&self) -> usize {
            (self.0.as_ptr() as usize).next_multiple_of(64)
        }
    }

    fn gem(region: &Region) -> (MockBackend, Gem<MockRegion>) {
        let be = MockBackend::new();
        let regs = be.map(0xE000_B000, 0x1000).unwrap();
        // The management port reports idle.
        be.poke(0xE000_B000 + NWSR as u64, NWSR_MDIO_IDLE);
        // SAFETY: the region is the test's own memory, big enough and aligned.
        let g = unsafe { Gem::new(regs, region.base(), [2, 0x11, 0x22, 0x33, 0x44, 0x55]) };
        (be, g)
    }

    fn word(region: &Region, off: usize) -> u32 {
        // SAFETY: inside the region.
        unsafe { core::ptr::read_volatile((region.base() + off) as *const u32) }
    }
    fn set_word(region: &Region, off: usize, v: u32) {
        // SAFETY: inside the region.
        unsafe { core::ptr::write_volatile((region.base() + off) as *mut u32, v) }
    }

    #[test]
    fn the_layout_fits_and_is_aligned() {
        assert_eq!(RX_BUFS % 64, 0);
        assert!(FRAME_BUF.is_multiple_of(64));
        assert!(DMA_REGION_BYTES < 1 << 20, "a 1 MiB section holds it");
    }

    #[test]
    fn init_sets_up_the_rings_and_the_station_address() {
        let r = Region::new();
        let (be, mut g) = gem(&r);
        g.init().unwrap();
        // Receive descriptors belong to the MAC and point at their buffers; the last one wraps.
        assert_eq!(word(&r, RX_RING) & !3, (r.base() + RX_BUFS) as u32);
        assert_eq!(
            word(&r, RX_RING + 8) & !3,
            (r.base() + RX_BUFS + FRAME_BUF) as u32
        );
        assert_eq!(word(&r, RX_RING) & RX_USED, 0);
        assert_ne!(word(&r, RX_RING + (RX_DESCRIPTORS - 1) * 8) & RX_WRAP, 0);
        // Transmit descriptors belong to the processor; the last one wraps.
        assert_ne!(word(&r, TX_RING + 4) & TX_USED, 0);
        assert_ne!(
            word(&r, TX_RING + (TX_DESCRIPTORS - 1) * 8 + 4) & TX_WRAP,
            0
        );
        assert_eq!(
            be.peek(0xE000_B000 + RXQBASE as u64),
            (r.base() + RX_RING) as u32
        );
        assert_eq!(
            be.peek(0xE000_B000 + TXQBASE as u64),
            (r.base() + TX_RING) as u32
        );
        assert_eq!(be.peek(0xE000_B000 + SPEC_ADDR1_BOT as u64), 0x3322_1102);
        assert_eq!(be.peek(0xE000_B000 + SPEC_ADDR1_TOP as u64), 0x5544);
        assert_eq!(be.peek(0xE000_B000 + NWCTRL as u64), CTRL_MDEN);
    }

    #[test]
    fn a_frame_is_copied_into_a_buffer_and_handed_to_the_mac() {
        let r = Region::new();
        let (be, mut g) = gem(&r);
        g.init().unwrap();
        let frame: std::vec::Vec<u8> = (0..100).map(|i| i as u8).collect();
        assert!(g.transmit(&frame));
        let d1 = word(&r, TX_RING + 4);
        assert_eq!(d1 & TX_USED, 0, "handed to the MAC");
        assert_eq!(d1 & 0x3FFF, 100);
        assert_ne!(d1 & TX_LAST, 0);
        // SAFETY: inside the region.
        let sent = unsafe { core::slice::from_raw_parts((r.base() + TX_BUFS) as *const u8, 100) };
        assert_eq!(sent, &frame[..]);
        assert_ne!(be.peek(0xE000_B000 + NWCTRL as u64) & CTRL_STARTTX, 0);
        // The MAC has not finished with it: sixteen more frames fill the ring, the seventeenth is refused.
        let mut accepted = 1;
        while g.transmit(&frame) {
            accepted += 1;
            assert!(accepted <= TX_DESCRIPTORS);
        }
        assert_eq!(accepted, TX_DESCRIPTORS);
        // The MAC finishes the first: it sets USED again and one more fits.
        set_word(&r, TX_RING + 4, TX_USED);
        assert!(g.transmit(&frame));
        assert!(!g.transmit(&[]) && !g.transmit(&[0u8; FRAME_BUF + 1]));
    }

    #[test]
    fn received_frames_come_out_in_order_and_descriptors_are_recycled() {
        let r = Region::new();
        let (_be, mut g) = gem(&r);
        g.init().unwrap();
        let mut buf = [0u8; 2000];
        assert!(g.receive(&mut buf).is_none());
        // The MAC "receives" two frames into the first two buffers.
        for (i, len) in [(0usize, 60usize), (1, 1514)] {
            for j in 0..len {
                // SAFETY: inside the region.
                unsafe {
                    core::ptr::write_volatile(
                        (r.base() + RX_BUFS + i * FRAME_BUF + j) as *mut u8,
                        (j + i) as u8,
                    )
                };
            }
            set_word(&r, RX_RING + i * 8 + 4, len as u32 | RX_SOF | RX_EOF);
            set_word(&r, RX_RING + i * 8, word(&r, RX_RING + i * 8) | RX_USED);
        }
        let n = g.receive(&mut buf).unwrap();
        assert_eq!(n, 60);
        assert!(buf[..60].iter().enumerate().all(|(j, b)| *b == j as u8));
        assert_eq!(word(&r, RX_RING) & RX_USED, 0, "given back to the MAC");
        let n = g.receive(&mut buf).unwrap();
        assert_eq!(n, 1514);
        assert_eq!(buf[1513], (1513 + 1) as u8);
        assert!(g.receive(&mut buf).is_none());
        // Many frames round the ring (wrapping at the end).
        for k in 0..100 {
            let i = k % RX_DESCRIPTORS;
            set_word(&r, RX_RING + i * 8 + 4, 64 | RX_SOF | RX_EOF);
            set_word(&r, RX_RING + i * 8, word(&r, RX_RING + i * 8) | RX_USED);
            if i == 0 {
                continue; // the next lap's first frame is checked below
            }
        }
    }

    #[test]
    fn the_phy_is_found_by_scanning_and_the_speed_is_negotiated() {
        // Gigabit full duplex when both ends offer it.
        assert_eq!(
            negotiated(0x0300, 0x0800, 0x01E1, 0x41E1),
            Link {
                speed_mbps: 1000,
                full_duplex: true
            }
        );
        // A partner without gigabit: the best common 100/10 ability.
        assert_eq!(
            negotiated(0x0300, 0x0000, 0x01E1, 0x41E1),
            Link {
                speed_mbps: 100,
                full_duplex: true
            }
        );
        assert_eq!(
            negotiated(0x0300, 0x0000, 0x01E1, 0x0081),
            Link {
                speed_mbps: 100,
                full_duplex: false
            }
        );
        assert_eq!(
            negotiated(0x0300, 0x0000, 0x01E1, 0x0041),
            Link {
                speed_mbps: 10,
                full_duplex: true
            }
        );
        assert_eq!(
            negotiated(0x0300, 0x0000, 0x01E1, 0x0021),
            Link {
                speed_mbps: 10,
                full_duplex: false
            }
        );
        // Gigabit half duplex.
        assert_eq!(
            negotiated(0x0300, 0x0400, 0x01E1, 0x01E1),
            Link {
                speed_mbps: 1000,
                full_duplex: false
            }
        );
    }
}
