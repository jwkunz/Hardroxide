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

## Async (feature `async`)

```rust,ignore
pub fn wait_async(self: &Arc<Self>, timeout: Option<Duration>) -> WaitFuture
```

Returns a future resolving to `Result<Option<u32>, IrqError>`. Each call
spawns a helper thread that blocks in `wait` and wakes the task, so it works
under any executor; it needs the `UioIrq` in an `Arc`.

**Status:** tested only against a fake descriptor. On the stock PYNQ image PL
interrupts need a `generic-uio` device-tree node per IP, which Hardroxide does
not generate yet ([G11](../08-verification-gaps.md)).
