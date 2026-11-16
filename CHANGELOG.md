# Changelog

## 0.1.0 (unreleased)

First release.

### Generator and CLI
- `hardroxide generate`, `update`, `inspect` for Zynq-7000 XSAs (Vivado 2025.2
  exercised; HWH from 2019.2 to 2025.2 parsed). Other architectures are refused.
- Deterministic output; interface hash (including the parameters generated driver
  constructors use) and bitstream hash, so `update` can swap a bitstream alone.
- Only the Processing System's address map is used: MicroBlaze-private
  peripherals no longer get PS addresses.
- Examples take an instance name when a design has several of a core.
- `inspect` reports what each instance becomes, with interrupt routing.

### Runtime
- Drivers: GPIO, HLS (`ap_ctrl_hs`/`ap_ctrl_chain`, pointer arguments), DMA
  (direct register mode), Timer, IIC, Quad SPI, UART Lite, BRAM controller,
  interrupt controller, AXI4-Stream FIFO.
- MMIO backends: `/dev/mem`, UIO, mock. DMA buffers over `u-dma-buf`.
- Interrupts over UIO (blocking and async), including fan-in through an AXI
  interrupt controller, with a generated device-tree overlay.
- `load()` refuses to reprogram the PL while a kernel driver is bound to a PL
  address.

### Fixed during hardware validation
- IIC: the status register is toggle-on-write; clearing it by writing `0xFF`
  set every bit and made every transfer report a NACK.
- DMA: a channel waiting on an empty stream ignored the stop request and stayed
  busy; the driver now soft-resets the core.
- FIFO: the received length is rounded up to whole words (documented).
- A bitstream-only update could leave stale DMA/SPI/Timer constructor values
  compiled in.

### Known limits
- Zynq-7000 only; Linux userspace; DMA scatter-gather, HLS stream and array
  ports, and IP with several register blocks are not supported.
- A GIC interrupt line keeps the trigger type of the first overlay that mapped it
  until reboot; on the validated image SPI 29 (`IRQ_F2P[0]`) is already mapped
  level-triggered at boot.
