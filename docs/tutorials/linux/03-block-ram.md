# Linux 3: Memory inside the FPGA

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 10 minutes · **Needs:** [Linux 1](01-lights.md)

An FPGA has small blocks of memory inside it (**block RAM**). An **AXI BRAM controller** gives the processor a window
onto one. The base overlay uses them as the program memories of its four MicroBlaze processors, and the processor loads
their programs through these windows:

```console
$ hardroxide inspect fixtures/board/pynq_base_overlay.xsa | grep bram
0x40000000      64 KiB  iop_pmoda_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x42000000      64 KiB  iop_pmodb_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x44000000      64 KiB  iop_arduino_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x46000000      64 KiB  iop_rpi_mb_bram_ctrl  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
```

> **Status:** new in 3.0: the program builds against the generated crate and has **not yet been run on a board**.

## 1. Fill a whole memory and read it back

The generated `bram_test` does that for any of them; with several in a design it takes the name:

```console
$ scripts/board/run.sh work/base bram_test iop_rpi_mb_bram_ctrl
65536 bytes: 0 mismatching words
```

(This output was captured on a PYNQ-Z2.)

## 2. Your own program: words and bytes

`AxiBram` also reads and writes **bytes**, handling the word-sized hardware access for you:

```rust,ignore
{{#include ../../../apps/base/linux/block_ram.rs}}
```

```console
$ cp apps/base/linux/block_ram.rs work/base/examples/
$ scripts/board/run.sh work/base block_ram
```

It ends with `BLOCK RAM OK`. The rules it shows:

- Offsets must be word aligned. A misaligned offset is an **error** (`Unaligned(5)`), not silently rounded.
- Writing 7 bytes at offset 4 changes bytes 4 to 10. Byte 11 lives in the same word as the last written bytes and keeps
  its value: the driver reads the word, replaces only your bytes, and writes it back.

## What you did

You used FPGA block RAM as a plain buffer of words and bytes.

Continue with [Linux 4: The whole overlay](04-base-overlay.md).
