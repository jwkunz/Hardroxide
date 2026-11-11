use crate::Design;
use sha2::{Digest, Sha256};
use std::fmt::Write;

impl Design {
    /// Canonical, version-stable text of the *interface*: addressable PL
    /// instances, their VLNVs, address windows, register maps and interrupt
    /// ports. Excludes IP parameters, descriptions, clocks and timestamps, so
    /// a Vivado upgrade or a bitstream-only change keeps the same text.
    pub fn interface_text(&self) -> String {
        let mut s = String::new();
        let mut insts: Vec<_> = self.instances.iter().filter(|i| !i.is_ps).collect();
        insts.sort_by(|a, b| a.name.cmp(&b.name));
        for i in insts {
            let _ = writeln!(s, "instance {} {}", i.name, i.vlnv);
            for b in &i.address_blocks {
                let _ = writeln!(s, " block {} {} {}", b.name, b.interface, b.range);
                for r in &b.registers {
                    let _ = writeln!(s, "  reg {} {:#x} {:?} {:#x}", r.name, r.offset, r.access, r.reset_value);
                    for f in &r.fields {
                        let _ = writeln!(s, "   field {} {} {} {:?}", f.name, f.bit_offset, f.bit_width, f.access);
                    }
                }
            }
            for p in &i.interrupts {
                let _ = writeln!(s, " irq {}", p.name);
            }
        }
        let mut rs: Vec<_> = self.memory_ranges.iter().collect();
        rs.sort_by(|a, b| (&a.instance, &a.address_block).cmp(&(&b.instance, &b.address_block)));
        for r in rs {
            let _ = writeln!(s, "range {} {} {:#x} {:#x}", r.instance, r.address_block, r.base, r.high);
        }
        s
    }

    /// Hex SHA-256 of [`interface_text`](Self::interface_text).
    pub fn interface_hash(&self) -> String {
        Sha256::digest(self.interface_text()).iter().map(|b| format!("{b:02x}")).collect()
    }
}
