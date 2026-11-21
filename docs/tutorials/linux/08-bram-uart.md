# Linux 8: Block RAM and UART

**Design:** `fixtures/pynq_z2_mem_uart.xsa` · **Time:** 10 minutes · **Needs:**
[setup](index.md#before-you-start-once)

Two simple but useful IP: an **AXI BRAM Controller** that gives the processor a
window onto 8 KiB of block RAM inside the FPGA (at `0x40000000`), and an **AXI UART
Lite** at `0x42c00000`.

```console
$ hardroxide inspect fixtures/pynq_z2_mem_uart.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: bc197828...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x40000000       8 KiB  axi_bram_ctrl_0  ->  hardroxide_ip::bram::AxiBram  (xilinx.com:ip:axi_bram_ctrl:4.1)
0x42c00000      64 KiB  axi_uartlite_0  ->  hardroxide_ip::uart::AxiUart  (xilinx.com:ip:axi_uartlite:2.0)
                           irq interrupt -> IRQ_F2P[0] (GIC SPI 29)

2 instances: 2 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_mem_uart.xsa -o work/mem --hardroxide-path .
```

## 1. Fill the whole memory and read it back

```console
$ scripts/board/run.sh work/mem bram_test
8192 bytes: 0 mismatching words
```

The generated `bram_test` writes a different value to every word of the RAM,
then reads all of it back and counts mismatches.

## 2. The memory as bytes

`AxiBram` also reads and writes **bytes**, handling the word-sized hardware access for
you. The rules are simple and the program shows them:

```rust,ignore
{{#include ../../../fixtures/board_tests/mem_uart/bram_bytes.rs}}
```

```console
$ cp fixtures/board_tests/mem_uart/bram_bytes.rs work/mem/examples/
$ scripts/board/run.sh work/mem bram_bytes
block RAM: 8192 bytes
bytes 0..16: [00, 00, 00, 00, 48, 45, 4c, 4c, 4f, 21, 21, 22, 33, 33, 33, 33]
ok: only bytes 4..11 changed
write at offset 5 -> Err(Unaligned(5))
```

- Offsets must be word aligned. A misaligned offset is an **error**
  (`Unaligned(5)`), not silently rounded.
- Writing 7 bytes at offset 4 changes bytes 4 to 10. Byte 11 (`0x22`) lives
  in the same word as the last written bytes, and keeps its value: the driver
  reads the word, replaces only your bytes, and writes it back.

## 3. The UART

```console
$ scripts/board/run.sh work/mem uart_hello
received 0 bytes: ""
errors: RxErrors { overrun: false, frame: false, parity: false }
```

`uart_hello` sends a greeting and prints whatever arrives within a second. With
nothing wired to the UART it receives nothing, which is what this shows; with
TX connected to RX it would print the greeting back. (UART and its interrupts
are exercised properly in [Linux 3](03-uart-iic-loopback.md), where two UARTs
are connected inside the FPGA.) The `errors:` line is the receiver's overrun,
frame and parity flags, all clear.

## What you did

You used FPGA block RAM as a plain byte buffer, with the driver taking care of the
word-sized access, and saw how a design with more IP than you need simply
generates a field for each.

Continue with [Linux 9: AXI4-Stream FIFO](09-stream-fifo.md).
