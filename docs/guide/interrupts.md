# Interrupts

Hardroxide turns each PL interrupt that is wired to the PS (`IRQ_F2P`, directly
or through `xlconcat`) into an `IrqLine` constant and a device-tree overlay that
exposes it to Linux as a UIO device. **None of this has been run on a board**
([Verification gaps, G11](../08-verification-gaps.md)).

## One-time setup on the board

1. Boot a kernel with `uio_pdrv_genirq` and add
   `uio_pdrv_genirq.of_id=generic-uio` to its command line.
2. Compile and apply the generated overlay (needs a base device tree built with
   symbols, so that `&intc` resolves):

```console
$ dtc -@ -I dts -O dtb -o interrupts.dtbo interrupts.dtso
$ sudo mkdir /sys/kernel/config/device-tree/overlays/hardroxide
$ sudo cp interrupts.dtbo /sys/kernel/config/device-tree/overlays/hardroxide/dtbo
```

3. Check it: `cargo run --example irq_probe` opens the UIO device of every
   interrupt line and reports any that are missing.

The overlay has one node per interrupt, so an IP with two interrupts (an AXI DMA
has `mm2s_introut` and `s2mm_introut`) gets two nodes sharing the same register
window. UIO devices are found by node name, falling back to the register base
address if it is unique.

## Waiting for an interrupt

```rust,ignore
use my_design::{Design, pac};
use std::time::Duration;

let d = Design::load()?;
let irq = pac::vadd_0::IRQ_INTERRUPT.open()?;   // UioIrq

d.vadd_0.enable_done_interrupt()?;               // GIER + IP_IER
d.vadd_0.set_a(40)?;
d.vadd_0.set_b(2)?;
d.vadd_0.start()?;
match irq.wait(Some(Duration::from_secs(1)))? {
    Some(_count) => {
        d.vadd_0.ack_interrupts()?;              // acknowledge BEFORE waiting again
        println!("{}", d.vadd_0.out_r()?);
    }
    None => println!("timed out"),
}
```

`UioIrq::wait` re-enables the interrupt, blocks until it fires (or the
timeout), and returns the cumulative event count. You must acknowledge the
device before waiting again, or the level-sensitive line fires immediately.

### AXI DMA

```rust,ignore
d.axi_dma_0.set_interrupts(true);                // IOC + error interrupts
let irq = pac::axi_dma_0::IRQ_MM2S_INTROUT.open()?;
let t = d.axi_dma_0.start(Direction::MemToStream, buf, len).map_err(|e| e.error)?;
irq.wait(Some(Duration::from_secs(1)))?;
let buf = t.wait(Duration::ZERO)?;               // collects the result, clears the flags
```

### Async

With the `async` feature of `hardroxide-irq`, wrap the `UioIrq` in an `Arc` and
`await` `wait_async(timeout)`; it works under any executor (see
[hardroxide-irq](../reference/irq.md#async-feature-async)).

## What the generator can and cannot trace

| Wiring | Result |
|---|---|
| IP output straight to PS7 `IRQ_F2P` | traced (bit 0) |
| IP output through one or more `xlconcat` blocks | traced (bit from the `INk_WIDTH` offsets) |
| Multi-bit output, fan-out, or other logic in between | `f2p_bit`/`gic_spi` are `None`; the line is skipped in the overlay |
| AXI interrupt controller (`axi_intc`) | not supported yet |

The `IRQ_F2P` bit maps to a GIC SPI per UG585 (bits 0-7: interrupt IDs 61-68,
bits 8-15: IDs 84-91), written into the overlay as `interrupts = <0 ID-32 4>`.
