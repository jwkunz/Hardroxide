//! MMU, caches and the L2 controller; cache maintenance for DMA. ARM only.
//!
//! [`enable`] gives the program a flat (virtual = physical) translation table of 1 MiB
//! sections and turns on the instruction and data caches, branch prediction and the PL310
//! L2 cache:
//!
//! | Region | Attributes |
//! |---|---|
//! | `0x0000_0000..0x2000_0000` (DDR) | normal, write-back write-allocate, cacheable |
//! | the `.dma` section (see [`hardroxide_dma`'s `static_dma_buffer!`]) | normal, **non-cacheable** |
//! | everything else (PL, PS peripherals, OCM) | strongly ordered |
//!
//! Buffers a PL master reads or writes belong in `.dma`: the CPU and the PL then see the same
//! memory with no cache maintenance. Memory outside it that a PL master touches needs
//! [`clean_range`] before the PL reads it and [`invalidate_range`] before the CPU reads what
//! the PL wrote.
//!
//! Single core: the translation table marks memory non-shareable and the SCU and `ACTLR.SMP`
//! stay off. Cache and table settings are the values the vendor's boot code uses (cross-checked
//! against `embeddedsw`; only the numbers are used).

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

/// Size of a translation-table section.
pub const SECTION: usize = 1 << 20;

const L2CC: usize = 0xF8F0_2000;
const L2_CTRL: usize = 0x100;
const L2_AUX: usize = 0x104;
const L2_TAG_LAT: usize = 0x108;
const L2_DATA_LAT: usize = 0x10C;
const L2_SYNC: usize = 0x730;
const L2_INV_PA: usize = 0x770;
const L2_CLEAN_PA: usize = 0x7B0;
const L2_CLEAN_INV_PA: usize = 0x7F0;
const L2_INV_WAY: usize = 0x77C;
const L2_ISR: usize = 0x21C;
const L2_IAR: usize = 0x220;
const SLCR: usize = 0xF800_0000;

// Section descriptors: domain 0, full access.
const NORMAL_WB: u32 = 0x1C0E; // TEX=001 C=1 B=1: write-back write-allocate
const NORMAL_NC: u32 = 0x1C02; // TEX=001 C=0 B=0: non-cacheable
const STRONGLY_ORDERED: u32 = 0x0C02;

#[repr(C, align(16384))]
struct Table([u32; 4096]);
static mut TABLE: Table = Table([0; 4096]);
static ENABLED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    static __dma_start: u8;
    static __dma_end: u8;
}

#[inline(always)]
fn reg_write(addr: usize, v: u32) {
    // SAFETY: fixed, always-present PS registers; callers pass the documented offsets.
    unsafe { core::ptr::write_volatile(addr as *mut u32, v) }
}

#[inline(always)]
fn reg_read(addr: usize) -> u32 {
    // SAFETY: as above.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

/// Whether [`enable`] has run.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Fill the translation table for a DDR of `ddr_bytes` (a multiple of 1 MiB) whose
/// `dma` byte range (1 MiB aligned) is non-cacheable.
fn fill_table(table: &mut [u32; 4096], ddr_bytes: usize, dma: core::ops::Range<usize>) {
    for (i, e) in table.iter_mut().enumerate() {
        let pa = (i << 20) as u32;
        let attrs = if i < ddr_bytes / SECTION {
            if dma.contains(&(i << 20)) {
                NORMAL_NC
            } else {
                NORMAL_WB
            }
        } else {
            STRONGLY_ORDERED
        };
        *e = pa | attrs;
    }
}

/// Invalidate the whole L1 data cache by set/way (without cleaning: contents are discarded).
///
/// # Safety
/// Discards dirty data; only for start-up, before the data cache is in use.
unsafe fn invalidate_l1_dcache() {
    // SAFETY: cache maintenance registers; caller guarantees no dirty data matters.
    unsafe {
        asm!("mcr p15, 2, {z}, c0, c0, 0", "isb", z = in(reg) 0u32, options(nostack));
        let ccsidr: u32;
        asm!("mrc p15, 1, {0}, c0, c0, 0", out(reg) ccsidr, options(nostack));
        let line_shift = (ccsidr & 7) + 4;
        let ways = ((ccsidr >> 3) & 0x3FF) + 1;
        let sets = ((ccsidr >> 13) & 0x7FFF) + 1;
        let way_shift = (ways - 1).leading_zeros();
        for set in 0..sets {
            for way in 0..ways {
                let v = (way << way_shift) | (set << line_shift);
                asm!("mcr p15, 0, {0}, c7, c6, 2", in(reg) v, options(nostack));
            }
        }
        asm!("dsb", options(nostack));
    }
}

fn l2_sync() {
    while reg_read(L2CC + L2_SYNC) & 1 != 0 {}
}

/// Invalidate and enable the L2 cache (if it is off).
unsafe fn enable_l2() {
    if reg_read(L2CC + L2_CTRL) & 1 != 0 {
        return;
    }
    // Aux control: all prefetching, round-robin-free replacement defaults the vendor uses.
    let aux = (reg_read(L2CC + L2_AUX) & 0xFFF1_FFFF) | 0x7236_0000;
    reg_write(L2CC + L2_AUX, aux);
    reg_write(L2CC + L2_TAG_LAT, 0x111);
    reg_write(L2CC + L2_DATA_LAT, 0x121);
    // The L2 RAM configuration in the SLCR (unlock, write, lock).
    reg_write(SLCR + 0x008, 0xDF0D);
    reg_write(SLCR + 0xA1C, 0x0002_0202);
    reg_write(SLCR + 0x004, 0x767B);
    reg_write(L2CC + L2_INV_WAY, 0xFFFF);
    while reg_read(L2CC + L2_INV_WAY) & 0xFFFF != 0 {}
    l2_sync();
    reg_write(L2CC + L2_IAR, reg_read(L2CC + L2_ISR));
    reg_write(L2CC + L2_CTRL, 1);
}

/// Turn on the MMU, the L1 caches, branch prediction and the L2 cache, with the memory
/// map in the module documentation. `ddr_bytes` is the size of the DDR (a multiple of 1 MiB,
/// at most 512 MiB). Idempotent.
///
/// # Safety
/// Call once, early (the generated `Design::init` does), with the CPU running from DDR at its
/// physical address and nothing else (a PL master, a debugger write) relying on the caches
/// being off. The `.dma` section must be 1 MiB aligned (the linker script template does it).
pub unsafe fn enable(ddr_bytes: usize) {
    if ENABLED.swap(true, Ordering::AcqRel) {
        return;
    }
    // SAFETY: the table is private to this function until the MMU is enabled; the linker
    // symbols bound the `.dma` section; the register writes follow the Cortex-A9 TRM
    // start-up sequence (invalidate, program the table, then enable).
    unsafe {
        let table = &mut *core::ptr::addr_of_mut!(TABLE);
        let dma =
            core::ptr::addr_of!(__dma_start) as usize..core::ptr::addr_of!(__dma_end) as usize;
        fill_table(&mut table.0, ddr_bytes.min(512 << 20), dma);

        enable_l2();

        asm!(
            // Invalidate instruction cache, branch predictor and TLBs.
            "mov {z}, #0",
            "mcr p15, 0, {z}, c7, c5, 0",
            "mcr p15, 0, {z}, c7, c5, 6",
            "mcr p15, 0, {z}, c8, c7, 0",
            "dsb",
            "isb",
            z = out(reg) _,
            options(nostack)
        );
        invalidate_l1_dcache();
        asm!(
            // TTBR0: table base, inner and outer write-back write-allocate, not shareable.
            "mcr p15, 0, {ttb}, c2, c0, 0",
            // TTBCR = 0 (TTBR0 only, 16 KiB table); DACR = client for every domain.
            "mov {t}, #0",
            "mcr p15, 0, {t}, c2, c0, 2",
            "mvn {t}, #0",
            "mcr p15, 0, {t}, c3, c0, 0",
            "isb",
            // SCTLR: M (MMU), C (data cache), Z (branch prediction), I (instruction cache).
            "mrc p15, 0, {t}, c1, c0, 0",
            "orr {t}, {t}, #0x1800",
            "orr {t}, {t}, #0x5",
            "dsb",
            "mcr p15, 0, {t}, c1, c0, 0",
            "isb",
            ttb = in(reg) (core::ptr::addr_of!(TABLE) as usize as u32) | 0x48,
            t = out(reg) _,
            options(nostack)
        );
    }
}

const LINE: usize = 32;

fn lines(addr: usize, len: usize) -> impl Iterator<Item = usize> {
    let start = addr & !(LINE - 1);
    let end = (addr + len + LINE - 1) & !(LINE - 1);
    (start..end).step_by(LINE)
}

/// Write cached data in `addr..addr+len` back to memory (L1 then L2): call before a PL
/// master reads a buffer outside `.dma`.
pub fn clean_range(addr: usize, len: usize) {
    for a in lines(addr, len) {
        // SAFETY: cache maintenance by address; no memory is modified.
        unsafe { asm!("mcr p15, 0, {0}, c7, c10, 1", in(reg) a, options(nostack)) };
    }
    // SAFETY: barrier.
    unsafe { asm!("dsb", options(nostack)) };
    if reg_read(L2CC + L2_CTRL) & 1 != 0 {
        for a in lines(addr, len) {
            reg_write(L2CC + L2_CLEAN_PA, a as u32);
        }
        l2_sync();
    }
}

/// Discard cached data in `addr..addr+len` (L2 then L1): call before the CPU reads what a PL
/// master wrote outside `.dma`. The range should be line (32 byte) aligned: a partial line's
/// neighbours are discarded too.
pub fn invalidate_range(addr: usize, len: usize) {
    if reg_read(L2CC + L2_CTRL) & 1 != 0 {
        for a in lines(addr, len) {
            reg_write(L2CC + L2_INV_PA, a as u32);
        }
        l2_sync();
    }
    for a in lines(addr, len) {
        // SAFETY: cache maintenance by address; the caller accepts the discard.
        unsafe { asm!("mcr p15, 0, {0}, c7, c6, 1", in(reg) a, options(nostack)) };
    }
    // SAFETY: barrier.
    unsafe { asm!("dsb", options(nostack)) };
}

/// Write back and discard `addr..addr+len`.
pub fn flush_range(addr: usize, len: usize) {
    for a in lines(addr, len) {
        // SAFETY: cache maintenance by address.
        unsafe { asm!("mcr p15, 0, {0}, c7, c14, 1", in(reg) a, options(nostack)) };
    }
    // SAFETY: barrier.
    unsafe { asm!("dsb", options(nostack)) };
    if reg_read(L2CC + L2_CTRL) & 1 != 0 {
        for a in lines(addr, len) {
            reg_write(L2CC + L2_CLEAN_INV_PA, a as u32);
        }
        l2_sync();
    }
}

#[cfg(test)]
mod tests {
    // `fill_table` is plain logic, but this module is ARM-only; see the QEMU test
    // (`baremetal/ps7-smoke`) for the behaviour.
}
