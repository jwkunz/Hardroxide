# Tutorial 3: UART and I2C without wires

**Design:** `fixtures/pynq_z2_loopback.xsa` · **Time:** 15 minutes · **Needs:**
[setup](index.md#before-you-start-once); the interrupt overlay for step 5

To test a serial port you normally need a jumper wire. This design wires the
peripherals to each other **inside the FPGA** instead: two AXI UART Lite cores with
their TX and RX crossed, and two AXI IIC cores on one shared open-drain bus. No
pins are used, so it runs on any board.

| Instance | Address | Role |
|---|---|---|
| `axi_uartlite_0`, `axi_uartlite_1` | `0x42c00000`, `0x42c10000` | TX of each goes to RX of the other |
| `axi_iic_0`, `axi_iic_1` | `0x41600000`, `0x41610000` | `axi_iic_1` plays an I2C target, `axi_iic_0` the controller |

```console
$ hardroxide inspect fixtures/pynq_z2_loopback.xsa
design_1 (zynq 7z020, Vivado 2025.2)
interface hash: fbad14d4...
FCLK0: 100000000 Hz

reachable from the PS (a field on the generated `Design`):
0x41600000      64 KiB  axi_iic_0  ->  hardroxide_ip::iic::AxiIic  (xilinx.com:ip:axi_iic:2.1)
                           irq iic2intc_irpt -> IRQ_F2P[2] (GIC SPI 31)
0x41610000      64 KiB  axi_iic_1  ->  hardroxide_ip::iic::AxiIic  (xilinx.com:ip:axi_iic:2.1)
                           irq iic2intc_irpt -> IRQ_F2P[3] (GIC SPI 32)
0x42c00000      64 KiB  axi_uartlite_0  ->  hardroxide_ip::uart::AxiUart  (xilinx.com:ip:axi_uartlite:2.0)
                           irq interrupt -> IRQ_F2P[0] (GIC SPI 29)
0x42c10000      64 KiB  axi_uartlite_1  ->  hardroxide_ip::uart::AxiUart  (xilinx.com:ip:axi_uartlite:2.0)
                           irq interrupt -> IRQ_F2P[1] (GIC SPI 30)

4 instances: 4 with a curated driver, 0 HLS wrapper(s), 0 raw register window(s)
$ hardroxide generate fixtures/pynq_z2_loopback.xsa -o work/loopback --hardroxide-path .
```

Because this design has *two* UARTs and *two* IICs, the generated examples
choose between them by name: `uart_hello` and `i2c_scan` take an instance name as
their first argument and default to the first one.

## 1. UART: 256 bytes each way

```rust,ignore
{{#include ../../fixtures/board_tests/loopback/uart_pair.rs}}
```

The UART Lite has a 16-byte FIFO, so the program sends 8 bytes, then reads 8 back,
rather than all 256 at once.

```console
$ cp fixtures/board_tests/loopback/uart_pair.rs work/loopback/examples/
$ scripts/board/run.sh work/loopback uart_pair
uart 0 -> 1: 256 bytes in 22.27681ms, match = true, errors = RxErrors { overrun: false, frame: false, parity: false }
uart 1 -> 0: 256 bytes in 22.325489ms, match = true, errors = RxErrors { overrun: false, frame: false, parity: false }
```

256 bytes in 22 ms is 11.5 kB/s, which is 115200 baud (10 bits per byte). That
the time matches the baud rate shows the data really went through the UART logic.

## 2. I2C: a controller talking to a target

```rust,ignore
{{#include ../../fixtures/board_tests/loopback/iic_pair.rs}}
```

Core 1 is made a target by writing its address (`0x2A << 1`) into its `ADR` register
(offset `0x110`, a raw register write: the driver implements the controller side
only). Core 0 then writes five bytes to address `0x2A`; the program reads them
back from core 1's receive FIFO. It also writes to an address nobody answers (the
driver reports `Nack`) and scans the whole bus.

```console
$ cp fixtures/board_tests/loopback/iic_pair.rs work/loopback/examples/
$ scripts/board/run.sh work/loopback iic_pair
target received [a5, 5a, 3c, ff, 00] (sent [a5, 5a, 3c, ff, 00]) match=true
write to 0x55 -> Err(Nack)
scan found [2a]
```

One detail worth knowing: this core holds the bus until its receive FIFO is read, so the
program queues the controller's bytes first and then drains the target's FIFO.

## 3. Interrupts

Apply this design's overlay as in [Tutorial 2](02-timer-spi-irq.md#4-apply-the-overlay), then
run the interrupt programs. Their output on a real board:

```console
$ cp fixtures/board_tests/loopback/uart_irq.rs fixtures/board_tests/loopback/iic_irq.rs work/loopback/examples/
$ scripts/board/run.sh work/loopback uart_irq
after enable (TX empty, nothing sent): event None
round 0: irq-driven RX byte Some(40) after 131.907µs
round 1: irq-driven RX byte Some(41) after 127.205µs
round 2: irq-driven RX byte Some(42) after 144.534µs
idle wait -> None
TX-empty interrupt on b: Some(8) after 128.035µs (a byte at 115200 baud takes ~87 us)
errors a RxErrors { overrun: false, frame: false, parity: false } b RxErrors { overrun: false, frame: false, parity: false }
total 566.904334ms
$ scripts/board/run.sh work/loopback iic_irq
ISR after init 0xd0
round 0: event Some(1), ISR 0xd2, NACK flag true
round 1: event Some(2), ISR 0xd2, NACK flag true
round 2: event Some(3), ISR 0xd2, NACK flag true
idle wait -> None
```

The UART interrupt is edge-triggered, and **this board image already claims the
interrupt line of `IRQ_F2P[0]`** for its own PL design as level-triggered, so
`axi_uartlite_0`'s UIO device fails with `EIO`. The program therefore uses
`axi_uartlite_1`. If you design your own system, put edge-triggered cores on
higher `IRQ_F2P` bits. The same applies to switching between this and the other
designs: reboot between designs whose overlays need different trigger types on one
line ([Interrupts](../guide/interrupts.md)).

## What you did

You exercised two UARTs and two I2C cores with real data, checked timing against the
baud rate, and used interrupts, all without a single wire. This design is also how
Hardroxide's own UART and IIC drivers were validated.

Continue with [Tutorial 4: An HLS kernel](04-hls-vadd.md).
