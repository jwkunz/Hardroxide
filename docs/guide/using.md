# Using a generated crate

```rust,ignore,fixture=pynq_z2_gpio
use my_design::Design;
use hardroxide_ip::gpio::Channel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Programs the PL, applies FCLK settings, maps every IP through /dev/mem.
    // Needs root on the board.
    let d = Design::load()?;
    d.axi_gpio_0.set_input_mask(Channel::One, 0)?; // channel 1: all outputs
    d.axi_gpio_0.write(Channel::One, 0b1010)?;
    println!("{:#x}", d.axi_gpio_0.read(Channel::Two)?);
    Ok(())
}
```

## What `Design` contains

`Design<B>` has one public field per addressable IP instance, named after the
Vivado instance (`axi_gpio_0`, `vadd_0`, ...). Its type is the driver for that
IP, already constructed over the IP's register window; `B` is the MMIO backend
(`DevMem` on the board, `MockBackend` in tests). Everything else about the
design lives in the `pac` module: base addresses, sizes, register offsets,
field masks and interrupt lines. The exact contents are listed in
[Generated crate](../reference/generated-crate.md).

Driver methods take `&self` where they only touch registers, so several parts of
your program can share one `&Design`. Methods that keep state in the driver
(SPI and I2C transactions, delays, DMA interrupt settings) take `&mut self`,
which the compiler enforces.

## Errors

`load()` returns `my_design::Error`: `AlreadyLoaded`, `Fpga(..)` (bitstream
hash, programming or clock setup failed) or `Mmio(..)` (mapping failed; usually
missing root). Each driver has its own error type (`MmioError` for plain
register access, `DmaIpError`, `IicError`, ...); all implement
`std::error::Error`, so `Box<dyn Error>` or `anyhow` collect them.

## Lifecycle

- `Design::load()` / `load_with` take a **process-wide singleton**: a second
  load in the same process returns `Error::AlreadyLoaded` until the first
  `Design` is dropped. This does not stop *another process* from reprogramming
  the PL (see [Verification gaps, G7](../08-verification-gaps.md)).
- `Design::map_only(backend)` maps the IP without programming anything and
  without the singleton. Use it with the mock backend in tests:

```rust,ignore,fixture=pynq_z2_gpio
# use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::MockBackend;

let be = MockBackend::new();
be.poke(0x4120_0008, 0b10_0101);           // pretend buttons are pressed
let d = my_design::Design::map_only(be.clone())?;
assert_eq!(d.axi_gpio_0.read(Channel::Two)?, 0b10_0101);
```

## Moving data with AXI DMA

DMA needs physically contiguous memory. With the `u-dma-buf` kernel module
loaded (one buffer per `/dev/udmabufN`), a loopback through the PL looks like
this. Each transfer takes its buffer by value and gives it back when the
hardware is done, so the CPU cannot touch memory the PL is still using:

```rust,ignore,fixture=pynq_z2_dma_loopback
# use my_design::Design;
# use hardroxide_dma::{DmaBuffer, Udmabuf};
# use hardroxide_ip::dma::Direction;
# use std::time::Duration;
# let d: Design<hardroxide_mmio::DevMem> = todo!();
let mut tx = Udmabuf::open(0)?;                     // /dev/udmabuf0
let rx = Udmabuf::open(1)?;
tx.as_mut_slice()[..4].copy_from_slice(&[1, 2, 3, 4]);
d.axi_dma_0.reset()?;
// Start receiving first, so the stream has somewhere to go.
let rx_t = d.axi_dma_0.start(Direction::StreamToMem, rx, 4).map_err(|e| e.error)?;
let tx_t = d.axi_dma_0.start(Direction::MemToStream, tx, 4).map_err(|e| e.error)?;
let tx = tx_t.wait(Duration::from_secs(1))?;       // buffers come back when done
let rx = rx_t.wait(Duration::from_secs(1))?;
assert_eq!(&rx.as_slice()[..4], &tx.as_slice()[..4]);
```

If `start` fails, the buffer comes back inside the error (`e.buffer`). If a
`Transfer` is dropped before it finishes, the channel is halted first; if it
will not halt, the buffer is deliberately leaked rather than freed while the
hardware may still write to it. See [AXI DMA](../reference/ip-dma.md).

## Peripherals through `embedded-hal`

The Timer, IIC and SPI drivers implement the `embedded-hal` 1.0 traits, so
existing device drivers (sensors, displays, flash) work on top of them. Add
`embedded-hal = "1"` to your own `Cargo.toml` to name the traits:

```rust,ignore,fixture=pynq_z2_peripherals
# use my_design::Design;
# let mut d: Design<hardroxide_mmio::DevMem> = todo!();
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;
use embedded_hal::spi::{MODE_0, SpiBus};

d.axi_timer_0.delay_ms(10);                         // DelayNs on the AXI Timer
d.axi_iic_0.init()?;
let mut id = [0u8; 1];
d.axi_iic_0.write_read(0x48, &[0x0F], &mut id)?;    // I2c: register read
d.axi_quad_spi_0.init(MODE_0, false, false)?;
let mut buf = [0x9F, 0, 0, 0];
d.axi_quad_spi_0.transfer_in_place(&mut buf)?;      // SpiBus: JEDEC ID read
```

## HLS kernels that read and write memory

Pointer arguments of an HLS kernel become 64-bit address registers. Give each
one a DMA buffer (for example a `Udmabuf`) and start the core:

```rust,ignore,fixture=pynq_z2_hls_scale
# use my_design::Design;
# use hardroxide_dma::{DmaBuffer, Udmabuf};
# use std::time::Duration;
# let d: Design<hardroxide_mmio::DevMem> = todo!();
let mut input = Udmabuf::open(0)?;
let output = Udmabuf::open(1)?;
for (i, w) in input.as_mut_slice().chunks_exact_mut(4).take(16).enumerate() {
    w.copy_from_slice(&(i as i32).to_le_bytes());
}
d.scale_0.set_in_r_buffer(&input)?;
d.scale_0.set_out_r_buffer(&output)?;
d.scale_0.set_n(16)?;
d.scale_0.set_k(3)?;
d.scale_0.start()?;
d.scale_0.wait_done(Duration::from_secs(1))?;
let out = &output.as_slice()[..4];                 // out[0] = in[0] * 3
```

## Testing without hardware

Every driver is written against the [`Mmio`](../reference/mmio.md) trait, so a
[`MockBackend`](../reference/mmio.md#mock-backend) can stand in for the board.
The mock supports pre-loading register values (`poke`), inspecting writes
(`peek`, `log`) and write-1-to-clear registers. It is a memory, not a model of
the IP: a register reads back what was last written or poked, so to test code
that waits for hardware (a done bit, a FIFO level) you poke the value the
hardware would produce, as in the example above. `MockBuffer` stands in for a
DMA buffer in the same way.
