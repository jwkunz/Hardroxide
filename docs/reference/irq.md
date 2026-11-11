# hardroxide-irq

Interrupt waiting over a Linux UIO device. Sync core; the async API needs the
`async` Cargo feature and has no executor dependency.

```rust,ignore
pub struct UioIrq
pub fn open(path: impl AsRef<Path>) -> Result<Self, IrqError>
pub fn from_file(file: File, path: impl Into<PathBuf>) -> Self
pub fn enable(&self) -> Result<(), IrqError>
pub fn wait(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError>
pub fn wait_event(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError>
```

UIO protocol: writing a 32-bit `1` unmasks the interrupt; a 4-byte read
returns the cumulative event count once it fires, and the kernel keeps the
line masked until re-enabled.

- `enable()` unmasks.
- `wait(timeout)` = `enable()` then `wait_event(timeout)`. Returns
  `Ok(Some(count))`, or `Ok(None)` on timeout (`None` timeout blocks forever).
  **Acknowledge the device** (for example `HlsIp::ack_interrupts`, or clearing
  the DMA status flags) before waiting again, or the line re-fires.
- `wait_event` waits without re-enabling.
- `from_file` wraps an already-open descriptor (tests use a socketpair).

### Waiting

```rust,ignore
pub trait IrqWaiter
fn enable(&self) -> Result<(), IrqError>;
fn wait_event(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError>;
```

`IrqWaiter` abstracts "unmask, then wait for the next event" and is
implemented by `UioIrq`. Drivers that multiplex one interrupt
([`AxiIntc::wait`](ip-intc.md)) are generic over it so tests can use a fake.

## Interrupt lines and UIO discovery

```rust,ignore
pub struct IrqLine
pub instance: &'static str,
pub port: &'static str,
pub f2p_bit: Option<u32>,
pub gic_spi: Option<u32>,
pub dt_node: &'static str,
pub base: u64,
pub via_intc: Option<(&'static str, u32)>,
pub fn open(&self) -> Result<UioIrq, IrqError>
pub fn open_in(&self, sysfs_uio: &Path, dev_dir: &Path) -> Result<UioIrq, IrqError>
pub fn find_uio(sysfs_uio: &Path, node: &str, base: u64) -> Result<Option<u32>, IrqError>
```

`IrqLine` describes one interrupt output of a PL IP; the generator emits one
constant per connected interrupt in the generated crate's `pac`
([Generated crate](generated-crate.md#pacrs)). `f2p_bit`/`gic_spi` come from
the HWH (`None` if the net could not be traced to `IRQ_F2P`), `dt_node` is the
name of the `uio` node in the generated `interrupts.dtso`, `base` the IP's
register base. `via_intc` is `Some((controller, input bit))` when the signal
feeds an [AXI Interrupt Controller](ip-intc.md); such a line has no UIO device
of its own, `f2p_bit`/`gic_spi` are `None`, and `open()` fails with
`IrqError::ViaController { instance, port, controller, bit }`. Open the
controller's own `IRQ_IRQ` line instead.

`find_uio` scans a `/sys/class/uio`-style tree and returns the device number
`N` of `uioN`: a device whose `name` equals `node` wins; otherwise the *only*
device whose `maps/map0/addr` equals `base`; otherwise `None` (also `None` if
the directory does not exist). `open()` uses `/sys/class/uio` and `/dev`;
`open_in` lets tests substitute both. If no device is found the error is
`IrqError::NotFound { instance, port, node, base }`, whose message points at
the overlay. Discovery behaviour on the real kernel is unverified
([G11](../08-verification-gaps.md)).

## Async (feature `async`)

```rust,ignore
pub fn wait_async(self: &Arc<Self>, timeout: Option<Duration>) -> WaitFuture
```

Returns a future resolving to `Result<Option<u32>, IrqError>`. Each call
spawns a helper thread that blocks in `wait` and wakes the task, so it works
under any executor; it needs the `UioIrq` in an `Arc`.

**Status:** tested only against a fake descriptor. On the board each interrupt
needs a `generic-uio` device-tree node; the generated crate's
`interrupts.dtso` provides them, but applying it on the stock PYNQ image is
unverified ([G11](../08-verification-gaps.md)).
