//! The processor-specific part of the register test: fill registers, spin until another task has run, compare.

use core::sync::atomic::AtomicU32;

fn pattern(seed: u64, i: usize) -> u8 {
    let mixed = (seed ^ (i as u64) << 24 ^ i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (mixed >> 29) as u8
}

/// Load x2..x17, x20..x28 and all of q0..q31 with values made from `seed`, wait until `other` changes (the other task
/// has run: it loads its own values into the same registers), and report whether every register still holds its value.
#[cfg(target_arch = "aarch64")]
pub fn survive_a_switch(seed: u64, other: &AtomicU32) -> bool {
    // 25 general registers (200 bytes, 8 of padding), then 32 vector registers; the values found again at 720.
    #[repr(C, align(16))]
    struct Buffer([u8; 1440]);
    let mut buffer = Buffer([0; 1440]);
    for (i, b) in buffer.0[..720].iter_mut().enumerate() {
        *b = pattern(seed, i);
    }
    let before = other.load(core::sync::atomic::Ordering::SeqCst);
    // SAFETY: loads from and stores to the buffer and reads the counter; every register written is declared.
    unsafe {
        core::arch::asm!(
            "ldp x2, x3, [x0, #0]",
            "ldp x4, x5, [x0, #16]",
            "ldp x6, x7, [x0, #32]",
            "ldp x8, x9, [x0, #48]",
            "ldp x10, x11, [x0, #64]",
            "ldp x12, x13, [x0, #80]",
            "ldp x14, x15, [x0, #96]",
            "ldp x16, x17, [x0, #112]",
            "ldp x20, x21, [x0, #128]",
            "ldp x22, x23, [x0, #144]",
            "ldp x24, x25, [x0, #160]",
            "ldp x26, x27, [x0, #176]",
            "ldr x28, [x0, #192]",
            "add x30, x0, #208",
            "ld1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x30], #64",
            "ld1 {{v4.16b, v5.16b, v6.16b, v7.16b}}, [x30], #64",
            "ld1 {{v8.16b, v9.16b, v10.16b, v11.16b}}, [x30], #64",
            "ld1 {{v12.16b, v13.16b, v14.16b, v15.16b}}, [x30], #64",
            "ld1 {{v16.16b, v17.16b, v18.16b, v19.16b}}, [x30], #64",
            "ld1 {{v20.16b, v21.16b, v22.16b, v23.16b}}, [x30], #64",
            "ld1 {{v24.16b, v25.16b, v26.16b, v27.16b}}, [x30], #64",
            "ld1 {{v28.16b, v29.16b, v30.16b, v31.16b}}, [x30], #64",
            "2:",
            "ldr w30, [x1]",
            "cmp w30, w18",
            "b.eq 2b",
            "add x30, x0, #720",
            "stp x2, x3, [x30, #0]",
            "stp x4, x5, [x30, #16]",
            "stp x6, x7, [x30, #32]",
            "stp x8, x9, [x30, #48]",
            "stp x10, x11, [x30, #64]",
            "stp x12, x13, [x30, #80]",
            "stp x14, x15, [x30, #96]",
            "stp x16, x17, [x30, #112]",
            "stp x20, x21, [x30, #128]",
            "stp x22, x23, [x30, #144]",
            "stp x24, x25, [x30, #160]",
            "stp x26, x27, [x30, #176]",
            "str x28, [x30, #192]",
            "add x30, x0, #928",
            "st1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x30], #64",
            "st1 {{v4.16b, v5.16b, v6.16b, v7.16b}}, [x30], #64",
            "st1 {{v8.16b, v9.16b, v10.16b, v11.16b}}, [x30], #64",
            "st1 {{v12.16b, v13.16b, v14.16b, v15.16b}}, [x30], #64",
            "st1 {{v16.16b, v17.16b, v18.16b, v19.16b}}, [x30], #64",
            "st1 {{v20.16b, v21.16b, v22.16b, v23.16b}}, [x30], #64",
            "st1 {{v24.16b, v25.16b, v26.16b, v27.16b}}, [x30], #64",
            "st1 {{v28.16b, v29.16b, v30.16b, v31.16b}}, [x30], #64",
            in("x0") buffer.0.as_mut_ptr(),
            in("x1") other.as_ptr(),
            inout("x18") u64::from(before) => _,
            out("x2") _, out("x3") _, out("x4") _, out("x5") _, out("x6") _, out("x7") _, out("x8") _, out("x9") _,
            out("x10") _, out("x11") _, out("x12") _, out("x13") _, out("x14") _, out("x15") _, out("x16") _, out("x17") _,
            out("x20") _, out("x21") _, out("x22") _, out("x23") _, out("x24") _, out("x25") _, out("x26") _, out("x27") _,
            out("x28") _, out("lr") _,
            out("v0") _, out("v1") _, out("v2") _, out("v3") _, out("v4") _, out("v5") _, out("v6") _, out("v7") _,
            out("v8") _, out("v9") _, out("v10") _, out("v11") _, out("v12") _, out("v13") _, out("v14") _, out("v15") _,
            out("v16") _, out("v17") _, out("v18") _, out("v19") _, out("v20") _, out("v21") _, out("v22") _, out("v23") _,
            out("v24") _, out("v25") _, out("v26") _, out("v27") _, out("v28") _, out("v29") _, out("v30") _, out("v31") _,
            options(nostack)
        );
    }
    buffer.0[..200] == buffer.0[720..920] && buffer.0[208..720] == buffer.0[928..1440]
}

/// The same with r3..r5, r8..r10, r12 and d0..d31.
#[cfg(target_arch = "arm")]
pub fn survive_a_switch(seed: u64, other: &AtomicU32) -> bool {
    // 7 general registers (28 bytes, 4 of padding), then 32 double registers; the values found again at 288.
    #[repr(C, align(8))]
    struct Buffer([u8; 576]);
    let mut buffer = Buffer([0; 576]);
    for (i, b) in buffer.0[..288].iter_mut().enumerate() {
        *b = pattern(seed, i);
    }
    let before = other.load(core::sync::atomic::Ordering::SeqCst);
    // SAFETY: loads from and stores to the buffer and reads the counter; every register written is declared.
    unsafe {
        core::arch::asm!(
            "ldr r3, [r0, #0]",
            "ldr r4, [r0, #4]",
            "ldr r5, [r0, #8]",
            "ldr r8, [r0, #12]",
            "ldr r9, [r0, #16]",
            "ldr r10, [r0, #20]",
            "ldr r12, [r0, #24]",
            "add lr, r0, #32",
            "vldmia lr!, {{d0-d15}}",
            "vldmia lr, {{d16-d31}}",
            "2:",
            "ldr lr, [r1]",
            "cmp lr, r2",
            "beq 2b",
            "str r3, [r0, #288]",
            "str r4, [r0, #292]",
            "str r5, [r0, #296]",
            "str r8, [r0, #300]",
            "str r9, [r0, #304]",
            "str r10, [r0, #308]",
            "str r12, [r0, #312]",
            "add lr, r0, #320",
            "vstmia lr!, {{d0-d15}}",
            "vstmia lr, {{d16-d31}}",
            in("r0") buffer.0.as_mut_ptr(),
            in("r1") other.as_ptr(),
            inout("r2") before => _,
            out("r3") _, out("r4") _, out("r5") _, out("r8") _, out("r9") _, out("r10") _, out("r12") _, out("lr") _,
            out("d0") _, out("d1") _, out("d2") _, out("d3") _, out("d4") _, out("d5") _, out("d6") _, out("d7") _,
            out("d8") _, out("d9") _, out("d10") _, out("d11") _, out("d12") _, out("d13") _, out("d14") _, out("d15") _,
            out("d16") _, out("d17") _, out("d18") _, out("d19") _, out("d20") _, out("d21") _, out("d22") _, out("d23") _,
            out("d24") _, out("d25") _, out("d26") _, out("d27") _, out("d28") _, out("d29") _, out("d30") _, out("d31") _,
            options(nostack)
        );
    }
    buffer.0[..28] == buffer.0[288..316] && buffer.0[32..288] == buffer.0[320..576]
}

/// Raise an undefined-instruction exception.
pub fn undefined_instruction() {
    // SAFETY: deliberately faults, to test the report.
    unsafe {
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(".inst 0x00000000");
        #[cfg(target_arch = "arm")]
        core::arch::asm!(".word 0xe7f000f0");
    }
}
