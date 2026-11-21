//! The Zynq-7000's SD/SDIO host controllers (`SDIO0`, `SDIO1`) driving an SD memory card in
//! 4-bit mode with programmed I/O: card initialisation (identification, RCA, 4-bit bus, clock) and
//! reading and writing 512-byte blocks.
//!
//! The controller follows the standard SD Host Controller interface, so the register layout is the
//! standard one (and the vendor driver's header agrees). Cards of every capacity class are handled:
//! standard capacity (byte addressing) and SDHC/SDXC (block addressing). Not supported: UHS-I
//! voltage switching and speeds, MMC/eMMC, SDIO cards, DMA (the data goes through the buffer data
//! port), and the high-speed switch (the bus runs at 25 MHz).

use core::time::Duration;
use hardroxide_mmio::block::{BLOCK_SIZE, BlockDevice};
use hardroxide_mmio::time::{Instant, sleep};
use hardroxide_mmio::{Mmio, MmioError};

// Registers (32-bit access; the sub-word registers share words).
const BLK: usize = 0x04; // block size [15:0], block count [31:16]
const ARG: usize = 0x08;
const MODE_CMD: usize = 0x0C; // transfer mode [15:0], command [31:16]
const RESP: usize = 0x10; // four words
const DATA: usize = 0x20;
const PRESENT: usize = 0x24;
const HOST: usize = 0x28; // host control [7:0], power [15:8]
const CLOCK: usize = 0x2C; // clock control [15:0], timeout [23:16], software reset [31:24]
const INT_STATUS: usize = 0x30; // normal [15:0], error [31:16]
const INT_ENABLE: usize = 0x34; // status enable: normal [15:0], error [31:16]
const INT_SIGNAL: usize = 0x38;

const PRES_CMD_INHIBIT: u32 = 1 << 0;
const PRES_DAT_INHIBIT: u32 = 1 << 1;
const PRES_CARD_INSERTED: u32 = 1 << 16;

const HOST_4BIT: u32 = 1 << 1;
const POWER_3V3_ON: u32 = 0x0F << 8;

const CLK_INT_EN: u32 = 1 << 0;
const CLK_INT_STABLE: u32 = 1 << 1;
const CLK_SD_EN: u32 = 1 << 2;
const SWRST_ALL: u32 = 1 << 24;
const SWRST_CMD: u32 = 1 << 25;
const SWRST_DAT: u32 = 1 << 26;

const INT_CMD_DONE: u32 = 1 << 0;
const INT_XFER_DONE: u32 = 1 << 1;
const INT_BUF_WRITE: u32 = 1 << 4;
const INT_BUF_READ: u32 = 1 << 5;
const INT_ERROR: u32 = 1 << 15;

const MODE_BLKCNT_EN: u32 = 1 << 1;
const MODE_AUTO_CMD12: u32 = 1 << 2;
const MODE_READ: u32 = 1 << 4;
const MODE_MULTI: u32 = 1 << 5;

const CMD_RESP_136: u32 = 0x01;
const CMD_RESP_48: u32 = 0x02;
const CMD_RESP_48_BUSY: u32 = 0x03;
const CMD_CRC: u32 = 1 << 3;
const CMD_INDEX: u32 = 1 << 4;
const CMD_DATA: u32 = 1 << 5;

/// The kind of response a command gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resp {
    None,
    /// 136-bit (R2: CID, CSD).
    R2,
    /// 48-bit with CRC and index checks (R1, R6, R7).
    R1,
    /// 48-bit, busy signalled afterwards (R1b).
    R1b,
    /// 48-bit without checks (R3: OCR).
    R3,
}

#[derive(Debug, thiserror::Error)]
pub enum SdError {
    #[error(transparent)]
    Mmio(#[from] MmioError),
    #[error("no card in the slot")]
    NoCard,
    #[error("the card did not answer (is it an SD card?)")]
    NoResponse,
    #[error("the card is not ready (initialisation timed out)")]
    NotReady,
    #[error("command {cmd} failed: error status {status:#06x}")]
    Command { cmd: u8, status: u16 },
    #[error("the transfer failed: error status {0:#06x}")]
    Transfer(u16),
    #[error("timed out waiting for the controller")]
    Timeout,
    #[error("the card was not initialised")]
    NotInitialised,
    #[error("blocks {lba}.. are past the end of the {blocks}-block card")]
    OutOfRange { lba: u64, blocks: u64 },
    #[error("a transfer must be a whole number of 512-byte blocks")]
    NotBlocks,
    #[error("the card reported an error in its status ({0:#010x})")]
    CardStatus(u32),
}

/// What identification found out about the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardInfo {
    /// Capacity in 512-byte blocks.
    pub blocks: u64,
    /// SDHC/SDXC (block addressed) rather than standard capacity.
    pub high_capacity: bool,
    /// The card's 4-byte-aligned product name, manufacturer id and serial number from the CID.
    pub manufacturer: u8,
    pub product: [u8; 5],
    pub serial: u32,
    /// The relative card address assigned.
    pub rca: u16,
    /// The bus clock in use, in hertz.
    pub clock_hz: u32,
}

/// `(divisor register value, achieved Hz)` for a bus clock of at most `target` from `base_hz`:
/// the register holds 0 (base clock) or a power of two up to 128, giving `base / (2 * value)`.
pub fn clock_divisor(base_hz: u32, target_hz: u32) -> (u32, u32) {
    if base_hz <= target_hz {
        return (0, base_hz);
    }
    let mut d = 1u32;
    while d < 128 && base_hz / (2 * d) > target_hz {
        d *= 2;
    }
    (d, base_hz / (2 * d))
}

/// The capacity in 512-byte blocks from a CSD register as the host controller presents it (the
/// 128-bit register with its CRC byte dropped, in four words, least significant first).
pub fn csd_blocks(r: &[u32; 4]) -> u64 {
    let structure = (r[3] >> 22) & 3;
    if structure == 1 {
        // CSD version 2 (SDHC/SDXC): C_SIZE in bits 69:48 of the card's register, 61:40 here.
        let c_size = u64::from((r[1] >> 8) & 0x3F_FFFF);
        (c_size + 1) * 1024
    } else {
        // Version 1: C_SIZE bits 73:62, READ_BL_LEN 83:80, C_SIZE_MULT 49:47 (65:54, 75:72, 41:39 here).
        let c_size = u64::from((r[1] >> 22) | ((r[2] & 3) << 10));
        let read_bl_len = (r[2] >> 8) & 0xF;
        let mult = (r[1] >> 7) & 7;
        let bytes = (c_size + 1) << (mult + 2) << read_bl_len;
        bytes / BLOCK_SIZE as u64
    }
}

pub struct SdHost<R: Mmio> {
    regs: R,
    base_hz: u32,
    info: Option<CardInfo>,
}

impl<R: Mmio> SdHost<R> {
    /// `base_hz` is the controller's input clock ([`crate::clocks::sdio_ref_hz`]).
    pub fn new(regs: R, base_hz: u32) -> Self {
        SdHost {
            regs,
            base_hz,
            info: None,
        }
    }

    pub fn regs(&self) -> &R {
        &self.regs
    }

    /// What identification found, once [`SdHost::init`] has succeeded.
    pub fn info(&self) -> Option<&CardInfo> {
        self.info.as_ref()
    }

    /// Whether the controller sees a card in the slot.
    pub fn card_present(&self) -> Result<bool, MmioError> {
        Ok(self.regs.read32(PRESENT)? & PRES_CARD_INSERTED != 0)
    }

    fn wait(&self, what: impl Fn(&R) -> Result<bool, MmioError>, ms: u64) -> Result<(), SdError> {
        let t0 = Instant::now();
        loop {
            if what(&self.regs)? {
                return Ok(());
            }
            if t0.elapsed() > Duration::from_millis(ms) {
                return Err(SdError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    fn reset(&self, bits: u32) -> Result<(), SdError> {
        let c = self.regs.read32(CLOCK)? & 0x00FF_FFFF;
        self.regs.write32(CLOCK, c | bits)?;
        self.wait(|r| Ok(r.read32(CLOCK)? & bits == 0), 100)
    }

    fn set_clock(&self, target_hz: u32) -> Result<u32, SdError> {
        let (d, hz) = clock_divisor(self.base_hz, target_hz);
        let timeout = self.regs.read32(CLOCK)? & 0x00FF_0000;
        // Stop the card clock, change the divisor, wait for the internal clock to settle, restart.
        self.regs.write32(CLOCK, timeout)?;
        self.regs.write32(CLOCK, timeout | d << 8 | CLK_INT_EN)?;
        self.wait(|r| Ok(r.read32(CLOCK)? & CLK_INT_STABLE != 0), 100)?;
        self.regs
            .write32(CLOCK, timeout | d << 8 | CLK_INT_EN | CLK_SD_EN)?;
        Ok(hz)
    }

    /// Issue a command and collect the response.
    fn command(&self, cmd: u8, arg: u32, resp: Resp, data: u32) -> Result<[u32; 4], SdError> {
        let r = &self.regs;
        let mut busy_mask = PRES_CMD_INHIBIT;
        if resp == Resp::R1b || data != 0 {
            busy_mask |= PRES_DAT_INHIBIT;
        }
        self.wait(|r| Ok(r.read32(PRESENT)? & busy_mask == 0), 100)?;
        r.write32(INT_STATUS, 0xFFFF_FFFF)?;
        let kind = match resp {
            Resp::None => 0,
            Resp::R2 => CMD_RESP_136 | CMD_CRC,
            Resp::R1 => CMD_RESP_48 | CMD_CRC | CMD_INDEX,
            Resp::R1b => CMD_RESP_48_BUSY | CMD_CRC | CMD_INDEX,
            Resp::R3 => CMD_RESP_48,
        };
        let command = u32::from(cmd) << 8 | kind | if data != 0 { CMD_DATA } else { 0 };
        r.write32(ARG, arg)?;
        // Transfer mode and command go out in one word: writing the command register starts it.
        r.write32(MODE_CMD, command << 16 | (data & 0xFFFF))?;
        let t0 = Instant::now();
        loop {
            let s = r.read32(INT_STATUS)?;
            if s & INT_ERROR != 0 {
                let status = (s >> 16) as u16;
                self.reset(SWRST_CMD | SWRST_DAT).ok();
                r.write32(INT_STATUS, 0xFFFF_FFFF)?;
                return Err(if status & 1 != 0 {
                    SdError::NoResponse
                } else {
                    SdError::Command { cmd, status }
                });
            }
            if s & INT_CMD_DONE != 0 {
                r.write32(INT_STATUS, INT_CMD_DONE)?;
                break;
            }
            if t0.elapsed() > Duration::from_millis(500) {
                self.reset(SWRST_CMD | SWRST_DAT).ok();
                return Err(SdError::Timeout);
            }
        }
        let out = [
            r.read32(RESP)?,
            r.read32(RESP + 4)?,
            r.read32(RESP + 8)?,
            r.read32(RESP + 12)?,
        ];
        if resp == Resp::R1b {
            // The card holds DAT0 low while it is busy; the controller reports transfer complete when it ends.
            self.wait_transfer_done(5000)?;
        }
        Ok(out)
    }

    fn wait_transfer_done(&self, ms: u64) -> Result<(), SdError> {
        let t0 = Instant::now();
        loop {
            let s = self.regs.read32(INT_STATUS)?;
            if s & INT_ERROR != 0 {
                self.reset(SWRST_CMD | SWRST_DAT).ok();
                self.regs.write32(INT_STATUS, 0xFFFF_FFFF)?;
                return Err(SdError::Transfer((s >> 16) as u16));
            }
            if s & INT_XFER_DONE != 0 {
                self.regs.write32(INT_STATUS, INT_XFER_DONE)?;
                return Ok(());
            }
            if t0.elapsed() > Duration::from_millis(ms) {
                self.reset(SWRST_CMD | SWRST_DAT).ok();
                return Err(SdError::Timeout);
            }
        }
    }

    /// An application command: CMD55 (with the card's address) then `cmd`.
    fn app_command(&self, rca: u16, cmd: u8, arg: u32, resp: Resp) -> Result<[u32; 4], SdError> {
        self.command(55, u32::from(rca) << 16, Resp::R1, 0)?;
        self.command(cmd, arg, resp, 0)
    }

    /// Reset the controller, identify the card and bring the bus to 4 bits and 25 MHz.
    pub fn init(&mut self) -> Result<CardInfo, SdError> {
        self.init_inner(false)
    }

    /// As [`SdHost::init`], but treat a card as present whatever the card-detect pin says (for a slot
    /// whose detect line is not routed to the controller).
    pub fn init_ignoring_card_detect(&mut self) -> Result<CardInfo, SdError> {
        self.init_inner(true)
    }

    fn init_inner(&mut self, force_present: bool) -> Result<CardInfo, SdError> {
        self.info = None;
        if !force_present && !self.card_present()? {
            return Err(SdError::NoCard);
        }
        self.reset(SWRST_ALL)?;
        if force_present {
            // Card detect from the test level (bit 7 selects the test signal, bit 6 is its level).
            let host = self.regs.read32(HOST)?;
            self.regs.write32(HOST, host | 3 << 6)?;
        }
        // Power the card at 3.3 V, enable the status bits we poll (everything, no interrupts to the CPU).
        let host = self.regs.read32(HOST)? & 0xFFFF_00FF;
        self.regs.write32(HOST, host | POWER_3V3_ON)?;
        self.regs.write32(INT_ENABLE, 0xFFFF_FFFF)?;
        self.regs.write32(INT_SIGNAL, 0)?;
        // Data timeout: the longest.
        let c = self.regs.read32(CLOCK)? & 0xFF00_FFFF;
        self.regs.write32(CLOCK, c | 0x000E_0000)?;
        // Identification runs at no more than 400 kHz.
        self.set_clock(400_000)?;
        sleep(Duration::from_millis(5));

        self.command(0, 0, Resp::None, 0)?;
        // CMD8: the card echoes the voltage and check pattern if it is a version 2 (or later) card.
        let v2 = match self.command(8, 0x1AA, Resp::R1, 0) {
            Ok(r) => r[0] & 0xFFF == 0x1AA,
            Err(SdError::NoResponse) => false,
            Err(e) => return Err(e),
        };
        // ACMD41 until the card reports it is no longer busy.
        let t0 = Instant::now();
        let ocr = loop {
            let arg = 0x00FF_8000 | if v2 { 1 << 30 } else { 0 };
            let r = self.app_command(0, 41, arg, Resp::R3)?;
            if r[0] & (1 << 31) != 0 {
                break r[0];
            }
            if t0.elapsed() > Duration::from_millis(1500) {
                return Err(SdError::NotReady);
            }
            sleep(Duration::from_millis(5));
        };
        let high_capacity = ocr & (1 << 30) != 0;

        let cid = self.command(2, 0, Resp::R2, 0)?;
        let rca = (self.command(3, 0, Resp::R1, 0)?[0] >> 16) as u16;
        let csd = self.command(9, u32::from(rca) << 16, Resp::R2, 0)?;
        self.command(7, u32::from(rca) << 16, Resp::R1b, 0)?;
        if !high_capacity {
            self.command(16, BLOCK_SIZE as u32, Resp::R1, 0)?;
        }
        // The four-bit bus.
        self.app_command(rca, 6, 2, Resp::R1)?;
        let host = self.regs.read32(HOST)?;
        self.regs.write32(HOST, host | HOST_4BIT)?;
        let clock_hz = self.set_clock(25_000_000)?;

        // The CID (register bits 127:8 as presented): manufacturer 119:112 here is the top byte of word 3.
        let info = CardInfo {
            blocks: csd_blocks(&csd),
            high_capacity,
            manufacturer: cid_manufacturer(&cid),
            product: cid_product(&cid),
            serial: cid_serial(&cid),
            rca,
            clock_hz,
        };
        self.info = Some(info);
        Ok(info)
    }

    fn address(&self, lba: u64) -> Result<u32, SdError> {
        let i = self.info.as_ref().ok_or(SdError::NotInitialised)?;
        Ok(if i.high_capacity {
            lba as u32
        } else {
            (lba * BLOCK_SIZE as u64) as u32
        })
    }

    fn check_range(&self, lba: u64, len: usize) -> Result<u32, SdError> {
        if !len.is_multiple_of(BLOCK_SIZE) || len == 0 {
            return Err(SdError::NotBlocks);
        }
        let blocks = self.info.as_ref().ok_or(SdError::NotInitialised)?.blocks;
        let n = (len / BLOCK_SIZE) as u64;
        if lba + n > blocks {
            return Err(SdError::OutOfRange { lba, blocks });
        }
        Ok(n as u32)
    }

    fn wait_buffer(&self, bit: u32) -> Result<(), SdError> {
        let t0 = Instant::now();
        loop {
            let s = self.regs.read32(INT_STATUS)?;
            if s & INT_ERROR != 0 {
                self.reset(SWRST_CMD | SWRST_DAT).ok();
                self.regs.write32(INT_STATUS, 0xFFFF_FFFF)?;
                return Err(SdError::Transfer((s >> 16) as u16));
            }
            if s & bit != 0 {
                self.regs.write32(INT_STATUS, bit)?;
                return Ok(());
            }
            if t0.elapsed() > Duration::from_millis(1000) {
                self.reset(SWRST_CMD | SWRST_DAT).ok();
                return Err(SdError::Timeout);
            }
        }
    }

    /// Read whole blocks into `buf`.
    pub fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), SdError> {
        let mut done = 0u64;
        let total = self.check_range(lba, buf.len())?;
        while done < u64::from(total) {
            // At most 65535 blocks per command (the block count register is 16 bits).
            let n = (u64::from(total) - done).min(0xFFFF) as u32;
            let part =
                &mut buf[done as usize * BLOCK_SIZE..(done as usize + n as usize) * BLOCK_SIZE];
            self.read_run(lba + done, part, n)?;
            done += u64::from(n);
        }
        Ok(())
    }

    fn read_run(&mut self, lba: u64, buf: &mut [u8], n: u32) -> Result<(), SdError> {
        let addr = self.address(lba)?;
        self.regs.write32(BLK, n << 16 | BLOCK_SIZE as u32)?;
        let (cmd, mode) = if n == 1 {
            (17, MODE_READ)
        } else {
            (
                18,
                MODE_READ | MODE_MULTI | MODE_BLKCNT_EN | MODE_AUTO_CMD12,
            )
        };
        self.command(cmd, addr, Resp::R1, mode | 1 << 31)?;
        for block in buf.chunks_mut(BLOCK_SIZE) {
            self.wait_buffer(INT_BUF_READ)?;
            for w in block.chunks_mut(4) {
                w.copy_from_slice(&self.regs.read32(DATA)?.to_le_bytes());
            }
        }
        self.wait_transfer_done(1000)
    }

    /// Write whole blocks from `buf`.
    pub fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), SdError> {
        let mut done = 0u64;
        let total = self.check_range(lba, buf.len())?;
        while done < u64::from(total) {
            let n = (u64::from(total) - done).min(0xFFFF) as u32;
            let part = &buf[done as usize * BLOCK_SIZE..(done as usize + n as usize) * BLOCK_SIZE];
            self.write_run(lba + done, part, n)?;
            done += u64::from(n);
        }
        Ok(())
    }

    fn write_run(&mut self, lba: u64, buf: &[u8], n: u32) -> Result<(), SdError> {
        let addr = self.address(lba)?;
        self.regs.write32(BLK, n << 16 | BLOCK_SIZE as u32)?;
        let (cmd, mode) = if n == 1 {
            (24, 0)
        } else {
            (25, MODE_MULTI | MODE_BLKCNT_EN | MODE_AUTO_CMD12)
        };
        self.command(cmd, addr, Resp::R1, mode | 1 << 31)?;
        for block in buf.chunks(BLOCK_SIZE) {
            self.wait_buffer(INT_BUF_WRITE)?;
            for w in block.chunks(4) {
                self.regs
                    .write32(DATA, u32::from_le_bytes([w[0], w[1], w[2], w[3]]))?;
            }
        }
        self.wait_transfer_done(5000)?;
        // The card is busy programming; wait until it is ready for data again (CMD13: ready bit 8).
        let rca = self.info.as_ref().map_or(0, |i| i.rca);
        let t0 = Instant::now();
        loop {
            let r = self.command(13, u32::from(rca) << 16, Resp::R1, 0)?[0];
            // The error bits of the card status (OUT_OF_RANGE, ADDRESS_ERROR, ..., ERROR).
            if r & 0xFDF9_8008 != 0 {
                return Err(SdError::CardStatus(r));
            }
            if r & (1 << 8) != 0 {
                return Ok(());
            }
            if t0.elapsed() > Duration::from_millis(2000) {
                return Err(SdError::Timeout);
            }
        }
    }
}

impl<R: Mmio> BlockDevice for SdHost<R> {
    type Error = SdError;

    fn block_count(&self) -> u64 {
        self.info.as_ref().map_or(0, |i| i.blocks)
    }

    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), SdError> {
        self.read(lba, buf)
    }

    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), SdError> {
        self.write(lba, buf)
    }
}

// The CID as the controller presents it: register bits 127:8 (the CRC byte is dropped), most significant word
// first in `cid[3]`. Fields (bit positions in the presented value): manufacturer 119:112, OEM 111:96,
// product name 95:56 (five characters), revision 55:48, serial number 47:16.
fn cid_manufacturer(cid: &[u32; 4]) -> u8 {
    (cid[3] >> 16) as u8
}

fn cid_product(cid: &[u32; 4]) -> [u8; 5] {
    [(cid[2] >> 24) as u8, (cid[2] >> 16) as u8, (cid[2] >> 8) as u8, cid[2] as u8, (cid[1] >> 24) as u8]
}

fn cid_serial(cid: &[u32; 4]) -> u32 {
    (cid[1] << 16) | (cid[0] >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cid_fields_are_decoded_from_the_right_bits() {
        // A real card (read on a Linux host from /sys/block/mmcblk0/device): manufacturer 0x02, OEM "TM"
        // (0x544d), product "SA32G", revision 0x11, serial 0x2df43e3b. Build the presented words from the
        // layout, with the month/year field in the low bits of word 0.
        let (mid, oem, name, prv, psn) = (0x02u32, 0x544du32, *b"SA32G", 0x11u32, 0x2df4_3e3bu32);
        let cid = [
            (psn << 16) | 0x0cc1, // serial bits 15:0, then the manufacturing date
            (u32::from(name[4]) << 24) | (prv << 16) | (psn >> 16),
            u32::from_be_bytes([name[0], name[1], name[2], name[3]]),
            (mid << 16) | oem,
        ];
        assert_eq!(cid_manufacturer(&cid), 0x02);
        assert_eq!(&cid_product(&cid), b"SA32G");
        assert_eq!(cid_serial(&cid), 0x2df4_3e3b);
    }

    #[test]
    fn the_clock_divisor_is_a_power_of_two() {
        // 50 MHz base: identification wants at most 400 kHz -> 50/(2*64) = 390.6 kHz.
        assert_eq!(clock_divisor(50_000_000, 400_000), (64, 390_625));
        assert_eq!(clock_divisor(50_000_000, 25_000_000), (1, 25_000_000));
        assert_eq!(clock_divisor(50_000_000, 50_000_000), (0, 50_000_000));
        assert_eq!(clock_divisor(100_000_000, 400_000), (128, 390_625));
        // Never above the target.
        for t in [400_000, 1_000_000, 10_000_000, 25_000_000] {
            assert!(clock_divisor(50_000_000, t).1 <= t);
        }
    }

    #[test]
    fn a_version_2_csd_gives_the_capacity() {
        // C_SIZE = 15 159 (a 7.4 GiB card): card bits 69:48 = 15159, bit 127:126 = 01.
        let c_size = 15_159u32;
        let r = [0, c_size << 8, 0, 1 << 22];
        assert_eq!(csd_blocks(&r), (u64::from(c_size) + 1) * 1024);
    }

    #[test]
    fn a_version_1_csd_gives_the_capacity() {
        // A 2 GB card: C_SIZE = 4095, C_SIZE_MULT = 7, READ_BL_LEN = 10 -> 4096 * 2^9 * 1024 bytes.
        let (c_size, mult, bl) = (4095u32, 7u32, 10u32);
        let r1 = (c_size & 0x3FF) << 22 | mult << 7;
        let r2 = (c_size >> 10) | bl << 8;
        assert_eq!(csd_blocks(&[0, r1, r2, 0]), 4096u64 * 512 * 1024 / 512);
    }
}
