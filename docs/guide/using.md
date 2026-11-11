# Using a generated crate

```rust,ignore
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

## Lifecycle

- `Design::load()` / `load_with` take a **process-wide singleton**: a second
  load in the same process returns `Error::AlreadyLoaded` until the first
  `Design` is dropped. This does not stop *another process* from reprogramming
  the PL (see [Verification gaps, G7](../08-verification-gaps.md)).
- `Design::map_only(backend)` maps the IP without programming anything and
  without the singleton. Use it with the mock backend in tests:

```rust,ignore
use hardroxide_mmio::MockBackend;

let be = MockBackend::new();
be.poke(0x4120_0008, 0b10_0101);           // pretend buttons are pressed
let d = my_design::Design::map_only(be.clone())?;
assert_eq!(d.axi_gpio_0.read(Channel::Two)?, 0b10_0101);
```

## Testing without hardware

Every driver is written against the [`Mmio`](../reference/mmio.md) trait, so a
[`MockBackend`](../reference/mmio.md#mock-backend) can stand in for the board.
The mock supports pre-loading register values (`poke`), inspecting writes
(`peek`, `log`) and write-1-to-clear registers.
