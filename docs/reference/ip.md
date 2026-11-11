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
| [`iic`](ip-iic.md) | AXI IIC | `i2c::I2c` |
| [`spi`](ip-spi.md) | AXI Quad SPI, standard mode | `spi::SpiBus`, `spi::SpiDevice` |

Common conventions:

- Constructors take the mapped region by value (`Driver::new(region, ...)`);
  `regs()` borrows it back for raw access, and extra constructor arguments come
  from the HWH (the generated `Design` fills them in).
- Methods that only touch registers return `Result<_, MmioError>`.
- Register offsets and control-bit positions are asserted against real Vivado
  HWH output in the unit tests; behaviour on real hardware is unverified
  ([G9–G12](../08-verification-gaps.md)).

The crate re-exports `Mmio` and `MmioError`.
