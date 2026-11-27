//! The MMU and caches; cache maintenance for DMA.
//!
//! `enable` (AArch64) gives the program a flat (virtual = physical) translation table
//! and turns on the instruction and data caches:
//!
//! | Region | Attributes |
//! |---|---|
//! | `0x0000_0000..0x8000_0000` (low DDR, as much as there is) | normal, write-back write-allocate, cacheable; 2 MiB blocks |
//! | the `.dma` section (see `hardroxide_dma`'s `static_dma_buffer!`) | normal, **non-cacheable** |
//! | `0x8_0000_0000..` (DDR above 2 GiB, whole GiB, up to 30) | normal, cacheable; 1 GiB blocks |
//! | everything else below 512 GiB (PL, PS peripherals, OCM) | device nGnRnE, never executed |
//!
//! Buffers a PL master or a PS peripheral reads or writes belong in `.dma`: the processor and
//! the device then see the same memory with no cache maintenance. Memory outside it that a
//! device touches needs `clean_range` before the device reads it and `invalidate_range`
//! before the processor reads what the device wrote.
//!
//! The table is built by a plain function, tested on the host.

/// Size of a translation block in low DDR, the unit of the `.dma` section and of
/// `set_non_cacheable`.
pub const SECTION: usize = 2 << 20;

const GIB: u64 = 1 << 30;
/// Where low DDR ends and where the rest of DDR appears.
const LOW_END: u64 = 2 * GIB;
const HIGH_BASE: u64 = 32 * GIB;
const HIGH_MAX: u64 = 30 * GIB;

/// Memory attributes by index (`MAIR_EL3`): device nGnRnE, normal write-back, normal
/// non-cacheable.
pub const MAIR: u64 = 0xFF << 8 | 0x44 << 16;

// Block descriptor bits: valid block, attribute index, AP[1] (reads as one at EL3),
// inner shareable, access flag, execute never.
const BLOCK: u64 = 0b01;
const AP1: u64 = 1 << 6;
const SHAREABLE: u64 = 0b11 << 8;
const ACCESSED: u64 = 1 << 10;
const NEVER_EXECUTE: u64 = 1 << 54;
const TABLE: u64 = 0b11;

const DEVICE: u64 = BLOCK | AP1 | ACCESSED | NEVER_EXECUTE;
const NORMAL_WB: u64 = BLOCK | 1 << 2 | AP1 | SHAREABLE | ACCESSED;
const NORMAL_NC: u64 = BLOCK | 2 << 2 | AP1 | SHAREABLE | ACCESSED | NEVER_EXECUTE;

/// A translation table of 512 entries.
#[derive(Debug)]
#[repr(C, align(4096))]
pub struct Table(pub [u64; 512]);

/// Fill the tables for a DDR of `ddr_bytes` whose `dma` byte range (aligned to
/// [`SECTION`], in low DDR) is non-cacheable. `level1` maps 1 GiB per entry; `low` maps the
/// first two GiB in 2 MiB blocks and `low_at` are the addresses the hardware finds those two
/// tables at.
pub fn fill_tables(
    level1: &mut Table,
    low: &mut [Table; 2],
    low_at: [u64; 2],
    ddr_bytes: u64,
    dma: core::ops::Range<u64>,
) {
    let low_ddr = ddr_bytes.min(LOW_END);
    let high_ddr = ddr_bytes.saturating_sub(LOW_END).min(HIGH_MAX);
    for (i, e) in level1.0.iter_mut().enumerate() {
        let at = i as u64 * GIB;
        *e = if i < 2 {
            low_at[i] | TABLE
        } else if at >= HIGH_BASE && at + GIB <= HIGH_BASE + high_ddr {
            at | NORMAL_WB
        } else {
            at | DEVICE
        };
    }
    for (t, table) in low.iter_mut().enumerate() {
        for (i, e) in table.0.iter_mut().enumerate() {
            let at = t as u64 * GIB + i as u64 * SECTION as u64;
            *e = at
                | if at >= low_ddr {
                    DEVICE
                } else if dma.contains(&at) {
                    NORMAL_NC
                } else {
                    NORMAL_WB
                };
        }
    }
}

#[cfg(target_arch = "aarch64")]
pub use hardware::{
    clean_range, enable, flush_range, invalidate_range, is_enabled, set_non_cacheable,
};

#[cfg(target_arch = "aarch64")]
mod hardware {
    use super::*;
    use core::arch::asm;
    use core::sync::atomic::{AtomicBool, Ordering};

    static mut LEVEL1: Table = Table([0; 512]);
    static mut LOW: [Table; 2] = [Table([0; 512]), Table([0; 512])];
    static ENABLED: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" {
        static __dma_start: u8;
        static __dma_end: u8;
    }

    // TCR_EL3: 39-bit addresses (lookup starts at level 1), table walks write-back and
    // inner shareable, 4 KiB granule, 40-bit physical addresses, reserved-one bits.
    const TCR: u64 = 25 | 1 << 8 | 1 << 10 | 3 << 12 | 2 << 16 | 1 << 23 | 1 << 31;

    /// Whether [`enable`] has run.
    pub fn is_enabled() -> bool {
        ENABLED.load(Ordering::Acquire)
    }

    /// Clean and invalidate every level of data cache by set and way.
    ///
    /// # Safety
    /// Only while the data cache is off (start-up).
    unsafe fn flush_all_data_caches() {
        // SAFETY: the architecture's sequence over CLIDR_EL1's levels; maintenance only.
        unsafe {
            asm!(
                "dmb sy",
                "mrs {clidr}, clidr_el1",
                "mov {level}, #0",              // level << 1, as CSSELR wants it
                "2:",
                "add {t}, {level}, {level}, lsr #1", // level * 3
                "lsr {t}, {clidr}, {t}",
                "and {t}, {t}, #7",
                "cmp {t}, #2",                  // 2 and up have a data or unified cache
                "b.lt 5f",
                "msr csselr_el1, {level}",
                "isb",
                "mrs {t}, ccsidr_el1",
                "and {line}, {t}, #7",
                "add {line}, {line}, #4",       // log2 of the line length
                "ubfx {ways}, {t}, #3, #10",    // ways - 1
                "ubfx {sets}, {t}, #13, #15",   // sets - 1
                "clz {wshift:w}, {ways:w}",
                "3:",                           // each set
                "mov {way}, {ways}",
                "4:",                           // each way
                "lsl {t}, {way}, {wshift}",
                "orr {t}, {t}, {level}",
                "lsl {u}, {sets}, {line}",
                "orr {t}, {t}, {u}",
                "dc cisw, {t}",
                "subs {way}, {way}, #1",
                "b.ge 4b",
                "subs {sets}, {sets}, #1",
                "b.ge 3b",
                "5:",
                "add {level}, {level}, #2",
                "cmp {level}, #14",
                "b.lt 2b",
                "msr csselr_el1, xzr",
                "dsb sy",
                "isb",
                clidr = out(reg) _, level = out(reg) _, t = out(reg) _, u = out(reg) _,
                line = out(reg) _, ways = out(reg) _, sets = out(reg) _, way = out(reg) _,
                wshift = out(reg) _,
                options(nostack)
            );
        }
    }

    /// Turn on the MMU and the caches with the memory map in the module documentation.
    /// `ddr_bytes` is the size of the DDR. Idempotent.
    ///
    /// # Safety
    /// Call once, early (the generated `Design::init` does), with the processor running at
    /// EL3 from DDR below 2 GiB and nothing else (a PL master, a debugger write) relying on
    /// the caches being off. The `.dma` section must be [`SECTION`] aligned (the linker
    /// script template does it).
    pub unsafe fn enable(ddr_bytes: usize) {
        if ENABLED.swap(true, Ordering::AcqRel) {
            return;
        }
        // SAFETY: the tables are private to this function until the MMU is enabled; the
        // linker symbols bound the `.dma` section; the register writes are the
        // architecture's enable sequence (invalidate, set up translation, then enable).
        unsafe {
            let level1 = &mut *core::ptr::addr_of_mut!(LEVEL1);
            let low = &mut *core::ptr::addr_of_mut!(LOW);
            let low_at = [
                core::ptr::addr_of!(low[0]) as u64,
                core::ptr::addr_of!(low[1]) as u64,
            ];
            let dma =
                core::ptr::addr_of!(__dma_start) as u64..core::ptr::addr_of!(__dma_end) as u64;
            fill_tables(level1, low, low_at, ddr_bytes as u64, dma);

            flush_all_data_caches();
            asm!(
                "tlbi alle3",
                "ic iallu",
                "dsb sy",
                "isb",
                "msr mair_el3, {mair}",
                "msr tcr_el3, {tcr}",
                "msr ttbr0_el3, {ttb}",
                "isb",
                // SCTLR_EL3: M (MMU), C (data cache), I (instruction cache).
                "mrs {t}, sctlr_el3",
                "orr {t}, {t}, #0x1000",
                "orr {t}, {t}, #0x4",
                "orr {t}, {t}, #0x1",
                "dsb sy",
                "msr sctlr_el3, {t}",
                "isb",
                mair = in(reg) MAIR,
                tcr = in(reg) TCR,
                ttb = in(reg) core::ptr::addr_of!(LEVEL1) as u64,
                t = out(reg) _,
                options(nostack)
            );
        }
    }

    /// The smallest data cache line in the system, in bytes.
    fn line() -> usize {
        let ctr: u64;
        // SAFETY: reads the cache type register.
        unsafe { asm!("mrs {0}, ctr_el0", out(reg) ctr, options(nomem, nostack)) };
        4 << ((ctr >> 16) & 0xF)
    }

    macro_rules! by_address {
        ($(#[$doc:meta])* $name:ident, $operation:literal) => {
            $(#[$doc])*
            pub fn $name(addr: usize, len: usize) {
                if len == 0 {
                    return;
                }
                let line = line();
                for a in (addr & !(line - 1)..addr + len).step_by(line) {
                    // SAFETY: cache maintenance by address.
                    unsafe { asm!(concat!("dc ", $operation, ", {0}"), in(reg) a, options(nostack)) };
                }
                // SAFETY: barrier.
                unsafe { asm!("dsb sy", options(nostack)) };
            }
        };
    }

    by_address!(
        /// Write cached data in `addr..addr+len` back to memory: call before a device
        /// reads a buffer outside `.dma`.
        clean_range,
        "cvac"
    );
    by_address!(
        /// Discard cached data in `addr..addr+len`: call before the processor reads what a
        /// device wrote outside `.dma`. The range should be line (64 byte) aligned: a
        /// partial line's neighbours are discarded too.
        invalidate_range,
        "ivac"
    );
    by_address!(
        /// Write back and discard `addr..addr+len`.
        flush_range,
        "civac"
    );

    /// Make the 2 MiB blocks covering `base..base+len` **non-cacheable** (normal memory, no
    /// caching), writing back and discarding anything cached there. For a buffer that a
    /// device reads or writes at a fixed address outside the `.dma` section. `base` and
    /// `len` are rounded out to whole blocks.
    ///
    /// # Panics
    /// If the range is not in DDR below 2 GiB.
    ///
    /// # Safety
    /// The range must be DDR that nothing else in the program uses as ordinary cached memory
    /// (code, data and stacks included), and [`enable`] must have run.
    pub unsafe fn set_non_cacheable(base: usize, len: usize) {
        let first = base / SECTION;
        let last = (base + len).div_ceil(SECTION);
        assert!(
            last <= LOW_END as usize / SECTION,
            "only DDR below 2 GiB can be made non-cacheable"
        );
        let entry = |i: usize| {
            // SAFETY: single core; the tables are only written here and in `enable`.
            unsafe {
                core::ptr::addr_of_mut!(LOW[i / 512])
                    .cast::<u64>()
                    .add(i % 512)
            }
        };
        let barrier = || {
            // SAFETY: make table writes visible, drop stale translations, synchronise.
            unsafe { asm!("dsb sy", "tlbi alle3", "dsb sy", "isb", options(nostack)) };
        };
        // Break before make: unmap, forget the old translations, then map anew.
        for i in first..last {
            // SAFETY: the caller guarantees nothing uses the range while it is unmapped.
            unsafe { entry(i).write_volatile(0) };
        }
        barrier();
        for i in first..last {
            // SAFETY: as above.
            unsafe { entry(i).write_volatile((i * SECTION) as u64 | NORMAL_NC) };
        }
        barrier();
        flush_range(first * SECTION, (last - first) * SECTION);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(ddr: u64) -> (Table, [Table; 2]) {
        let mut l1 = Table([0; 512]);
        let mut low = [Table([0; 512]), Table([0; 512])];
        fill_tables(
            &mut l1,
            &mut low,
            [0x1000, 0x2000],
            ddr,
            0x20_0000..0x60_0000,
        );
        (l1, low)
    }

    fn attributes(e: u64) -> u64 {
        e & !0x0000_FFFF_FFE0_0000
    }

    #[test]
    fn four_gib_board() {
        let (l1, low) = tables(4 * GIB);
        assert_eq!(l1.0[0], 0x1000 | TABLE);
        assert_eq!(l1.0[1], 0x2000 | TABLE);
        // Low DDR is cacheable, but for the DMA section.
        assert_eq!(low[0].0[0], NORMAL_WB);
        assert_eq!(low[0].0[1], 0x20_0000 | NORMAL_NC);
        assert_eq!(low[0].0[2], 0x40_0000 | NORMAL_NC);
        assert_eq!(low[0].0[3], 0x60_0000 | NORMAL_WB);
        assert_eq!(low[1].0[511], 0x7FE0_0000 | NORMAL_WB);
        // The PL windows and peripherals are device memory.
        assert_eq!(l1.0[2], 0x8000_0000 | DEVICE);
        assert_eq!(l1.0[3], 0xC000_0000 | DEVICE);
        assert_eq!(l1.0[16], (16 * GIB) | DEVICE);
        // The upper 2 GiB of DDR at 32 GiB, and nothing after it.
        assert_eq!(l1.0[31], (31 * GIB) | DEVICE);
        assert_eq!(l1.0[32], (32 * GIB) | NORMAL_WB);
        assert_eq!(l1.0[33], (33 * GIB) | NORMAL_WB);
        assert_eq!(l1.0[34], (34 * GIB) | DEVICE);
        assert_eq!(l1.0[511], (511 * GIB) | DEVICE);
    }

    #[test]
    fn less_than_two_gib_leaves_the_rest_as_device_memory() {
        let (l1, low) = tables(GIB / 2);
        assert_eq!(low[0].0[255], 0x1FE0_0000 | NORMAL_WB);
        assert_eq!(low[0].0[256], 0x2000_0000 | DEVICE);
        assert_eq!(low[1].0[0], 0x4000_0000 | DEVICE);
        assert_eq!(l1.0[32], (32 * GIB) | DEVICE);
    }

    #[test]
    fn descriptor_bits() {
        let (l1, _) = tables(4 * GIB);
        // Device memory is attribute 0 and cannot be executed; normal memory can.
        assert_eq!(attributes(l1.0[2]), NEVER_EXECUTE | 0x441);
        assert_eq!(attributes(l1.0[32]), 0x745);
        assert_eq!((MAIR >> 8) & 0xFF, 0xFF);
        assert_eq!((MAIR >> 16) & 0xFF, 0x44);
        assert_eq!(MAIR & 0xFF, 0);
    }
}
