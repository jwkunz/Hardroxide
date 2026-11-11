# hardroxide-ip

Curated drivers for Xilinx IP, written against the
[`Mmio`](mmio.md#traits) trait so they run on any backend, including the mock.
Drivers do not touch the FPGA or clocks; they only drive registers.

| Module | IP | `embedded-hal` 1.0 |
|---|---|---|
| [`gpio`](ip-gpio.md) | AXI GPIO | not yet |
| [`hls`](ip-hls.md) | Vitis HLS IP (`ap_ctrl_hs` / `ap_ctrl_chain` control block) | no |
| [`dma`](ip-dma.md) | AXI DMA, direct register mode | no |
| [`timer`](ip-timer.md) | AXI Timer | `delay::DelayNs` |
| [`bram`](ip-bram.md) | AXI BRAM Controller | no |
| [`uart`](ip-uart.md) | AXI UART Lite | `std::io::Write` |
| [`fifo`](ip-fifo.md) | AXI4-Stream FIFO (AXI4-Lite mode) | no |
| [`intc`](ip-intc.md) | AXI Interrupt Controller (fan-in) | no |
| [`iic`](ip-iic.md) | AXI IIC | `i2c::I2c` |
| [`spi`](ip-spi.md) | AXI Quad SPI, standard mode | `spi::SpiBus`, `spi::SpiDevice` |

Common conventions:

- Constructors take the mapped region by value (`Driver::new(region, ...)`);
  `regs()` borrows it back for raw access, and extra constructor arguments come
  from the HWH (the generated `Design` fills them in from
  [`Instance::driver_config`](hwh.md), which is part of the interface hash).
- Drivers are plain structs over an `Mmio` region, so they also work outside a
  generated crate: map a window yourself (`DevMem::new().map(base, size)`) and
  call `AxiGpio::new(region)`.
- Waiting is by polling with a timeout unless the method takes an interrupt
  waiter ([`AxiIntc::wait`](ip-intc.md)); use [`hardroxide-irq`](irq.md) to
  sleep until an interrupt instead.
- Methods that only touch registers return `Result<_, MmioError>`.
- Register offsets and control-bit positions are asserted against real Vivado
  HWH output in the unit tests; behaviour on real hardware is unverified
  ([G5, G9–G15](../08-verification-gaps.md)).

The crate re-exports `Mmio` and `MmioError`.
