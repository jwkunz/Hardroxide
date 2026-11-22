// Render the linker script from hardroxide-ps7's template. (A generated crate gets the
// finished script written by `hardroxide generate`; this is the same substitution.)
use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    // DDR from 1 MiB (the first MiB is left for the boot ROM/OCM aliasing), 64 MiB.
    let script = hardroxide_ps7::LINKER_SCRIPT_TEMPLATE
        .replace("@RAM_ORIGIN@", "0x00100000")
        .replace("@RAM_LENGTH@", "0x04000000")
        .replace("@STACK_FIQ@", "0x400")
        .replace("@STACK_IRQ@", "0x2000")
        .replace("@STACK_ABT@", "0x400")
        .replace("@STACK_UND@", "0x400")
        .replace("@STACK_SVC@", "0x1000")
        .replace("@STACK_SYS@", "0x100000");
    assert!(
        !script.contains('@'),
        "unfilled placeholder in the linker script"
    );
    fs::write(out.join("link.ld"), script).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-Tlink.ld");
    println!("cargo:rerun-if-changed=build.rs");
}
