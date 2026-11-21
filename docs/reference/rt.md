# hardroxide-rt

A FreeRTOS-style real-time kernel for the Zynq-7000's Cortex-A9, written in this repository
with no third-party dependencies ([ADR 0003](../decisions/0003-bare-metal-and-rtos-platforms.md)).
`no_std`, single core, static allocation. The scheduling policy (`sched`) is plain Rust, unit-tested on
the host; the context switch and tick (`port`, `kernel`) exist only for `target_arch = "arm"`.

Verified on a PYNQ-Z2 and under QEMU with `baremetal/rt-smoke` (priority preemption, periodic release
without drift, queues, interrupt-to-task hand-off, priority inheritance resolving an inversion);
see [Verification gaps](../08-verification-gaps.md).

The kernel supplies the IRQ and SVC vector entries, so a program using it depends on
`hardroxide-ps7` with `default-features = false` (otherwise the link fails with duplicate symbols).

## Model

**Priorities: a higher number is more urgent**; `0` is the idle task (the kernel's own), user tasks use 1 and up.
The most urgent ready task runs; equals share the processor round-robin, one tick each; a more urgent task
that becomes ready preempts at once. Tasks have their own stacks (`task_stack!`), at least 512 bytes. All state
is static: at most 16 tasks and 32 semaphores/mutexes. Interrupts are masked only inside the kernel's short
critical sections.

```rust,ignore
pub fn spawn(name: &'static str, prio: u8, stack: &'static mut [u8], entry: fn()) -> Result<TaskId, Error>
pub fn start(periph_hz: u64, tick_hz: u32) -> !
macro_rules! task_stack
```

`spawn` may be called before `start` or from a task. `start(periph_hz, tick_hz)` creates the idle task, starts the tick (the
private timer, `periph_hz` from `hardroxide_ps7::clocks::periph_hz`) and runs the most urgent task; the GIC must already be
initialised (the generated `Design::init` does it). A task function that returns ends the task. Floating point
may be used in tasks and handlers: the context switch saves `d0`-`d15` and `FPSCR`.

## Time

```rust,ignore
pub fn ms_to_ticks(ms: u32) -> u64
pub fn ticks() -> u64
pub fn delay(ticks: u64)
pub fn delay_ms(ms: u32)
pub fn yield_now()
pub struct Periodic
pub fn every_ms(ms: u32) -> Self
pub fn wait_next(&mut self) -> bool
pub const WAIT_FOREVER: u64
```

Timeouts are in ticks; `WAIT_FOREVER` waits without limit and `0` polls. `Periodic` is a periodic release: build it at
the top of a task, call `wait_next` at the end of each iteration; the period does not drift however long an
iteration takes, and `wait_next` returns `false` (without sleeping) when an iteration overran its period.

## Synchronisation

```rust,ignore
pub struct Semaphore
pub const fn new(initial: u32, max: u32) -> Self
pub fn take(&self, timeout: u64) -> bool
pub fn give(&self)
pub fn give_from_isr(&self)
pub struct Mutex
pub fn lock(&self, timeout: u64) -> Option<MutexGuard<'_>>
pub struct Queue<T: Copy, const N: usize>
pub fn send(&self, v: T, timeout: u64) -> bool
pub fn recv(&self, timeout: u64) -> Option<T>
pub fn send_from_isr(&self, v: T) -> bool
```

All are `const` to construct, so they can be `static`s. A semaphore given while a task waits hands the permit straight to the most
urgent waiter (the longest-waiting among equals). **`Mutex` has priority inheritance**: while a more urgent task waits, the holder runs
at that task's priority (along a chain of holders), so a medium-priority task cannot starve it; the priority drops back on
release. `MutexGuard` unlocks on drop and is not recursive. The `*_from_isr` calls never switch tasks themselves; the switch
happens as the interrupt returns, if a more urgent task became ready. Interrupt handlers are the plain functions of
[`hardroxide_ps7::irq`](ps7.md#interrupt-handlers) (they run on the IRQ stack).

## Sharing resources between tasks

```rust,ignore
pub struct Shared<T>
pub fn set(&self, v: T)
pub fn lock(&self, timeout: u64) -> Option<SharedGuard<'_, T>>
```

`Shared<T>` is a value behind a priority-inheriting mutex: a `static Shared<Design<PhysMem>>`, filled with `set` in `main`
before `start`, lets several tasks use the generated `Design`; `lock` returns a guard that derefs to `&mut T` and unlocks on drop.

## Generated RTOS crates

`hardroxide generate design.xsa --platform rtos` writes the [bare-metal crate](baremetal.md) (same `Design`, `pac`, `ips`,
`link.ld`) with `hardroxide-rt` as a dependency, `hardroxide-ps7` without its default interrupt entries, `hardroxide_rt`
re-exported as `rt`, and `periph_hz()` (the tick source's clock). It has one example, `examples/rt_control.rs`: a 10 ms
control task (priority 5) that takes the `Design` from a `Shared`, and a reporting task. Only kernel programs link in
this flavour, so the bare-metal examples are not generated. On the PYNQ-Z2 the example's control loop reported a worst
deviation of 1 us from its 10 ms period over 700 releases. `hardroxide update` handles these crates like bare-metal ones.

## Not yet

Event flags and task notifications, software timers, task suspend/delete from other tasks, GIC-priority-mask critical
sections (so urgent interrupts are never delayed by the kernel), a tickless idle, and a blocking `IrqWait` integration (today an
interrupt is turned into a semaphore give by a handler).
