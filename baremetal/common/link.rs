// The build script of the programs that run on both families: renders the linker script of the target's support crate.
// (A generated crate gets the finished script written by `hardroxide generate`; this is the same substitution.)
use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    // DDR from 1 MiB, 64 MiB.
    let script = if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64" {
        hardroxide_psu::LINKER_SCRIPT_TEMPLATE
            .replace("@STACK_EXCEPTION@", "0x4000")
            .replace("@STACK_MAIN@", "0x10000")
    } else {
        hardroxide_ps7::LINKER_SCRIPT_TEMPLATE
            .replace("@STACK_FIQ@", "0x400")
            .replace("@STACK_IRQ@", "0x2000")
            .replace("@STACK_ABT@", "0x400")
            .replace("@STACK_UND@", "0x400")
            .replace("@STACK_SVC@", "0x1000")
            .replace("@STACK_SYS@", "0x10000")
    }
    .replace("@RAM_ORIGIN@", "0x00100000")
    .replace("@RAM_LENGTH@", "0x04000000");
    assert!(
        !script.contains('@'),
        "unfilled placeholder in the linker script"
    );
    fs::write(out.join("link.ld"), script).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-Tlink.ld");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../common/link.rs");
}
