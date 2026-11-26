# hardroxide-rt

A FreeRTOS-style real-time kernel for the Zynq-7000's Cortex-A9 and, new in 3.0, the Cortex-A53 of Zynq UltraScale+, written
in this repository with no third-party dependencies ([ADR 0003](../decisions/0003-bare-metal-and-rtos-platforms.md),
[ADR 0005](../decisions/0005-ultrascale-plus.md)). `no_std`, single core, static allocation. The scheduling policy (`sched`)
is plain Rust, unit-tested on the host, and the kernel API is the same on both; the context switch (`port`) and the tick
depend on the target, which chooses them.

Verified under QEMU and on a board with `baremetal/rt-smoke`, one program for both processors: priority preemption, periodic
release without drift, queues, interrupt-to-task hand-off, priority inheritance resolving an inversion, registers intact
across task switches (the Cortex-A53 on an RFSoC 4x2; the Cortex-A9 on a PYNQ-Z2 as of 2.7); see
[Verification gaps](../08-verification-gaps.md).

The kernel supplies the interrupt and supervisor-call entries, so a program using it depends on the processor's support crate
with `default-features = false` (otherwise the link fails with duplicate symbols):
[`hardroxide-ps7`](ps7.md) on the Zynq-7000, [`hardroxide-psu`](psu.md) on UltraScale+.

## Model

**Priorities: a higher number is more urgent**; `0` is the idle task (the kernel's own), user tasks use 1 and up.
The most urgent ready task runs; equals share the processor round-robin, one tick each; a more urgent task
that becomes ready preempts at once. Tasks have their own stacks (`task_stack!`), at least 512 bytes on the Zynq-7000 and 2048
on UltraScale+ (a saved context there is 800 bytes). All state is static: at most 16 tasks and 32 semaphores/mutexes.
Interrupts are masked only inside the kernel's short critical sections.

```rust,ignore
pub fn spawn(name: &'static str, prio: u8, stack: &'static mut [u8], entry: fn()) -> Result<TaskId, Error>
pub fn start(periph_hz: u64, tick_hz: u32) -> !
macro_rules! task_stack
```

`spawn` may be called before `start` or from a task. `start(periph_hz, tick_hz)` creates the idle task, starts the tick and
runs the most urgent task; the GIC must already be initialised (the generated `Design::init` does it). `periph_hz` is the clock
of the tick's timer: on the Zynq-7000 the private timer's (`hardroxide_ps7::clocks::periph_hz`), on UltraScale+ the system
counter's (`hardroxide_psu::clocks::timestamp_hz`; the timer is the secure physical timer). A task function that returns ends
the task. Floating point may be used in tasks and handlers: the context switch saves `d0`-`d15` and `FPSCR` on the Cortex-A9,
all of `q0`-`q31`, `FPCR` and `FPSR` on the Cortex-A53.

**Before `start`** no task runs: `main` may create semaphores, give them and take what is there, and enable interrupts (their
handlers run and return to `main`). A call that would have to wait returns at once with its failure value, and `delay` does
nothing. (2.x could switch to a task here, abandoning `main` until every task blocked.)

On UltraScale+ tasks run at EL3 on `SP_EL0` and the handlers on the linker script's exception stack.

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
[`hardroxide_ps7::irq`](ps7.md#interrupt-handlers) or `hardroxide_psu::irq` (they run on the interrupt, or exception, stack and
must not unmask interrupts).

## Sleeping on a hardware interrupt

```rust,ignore
pub struct IrqEvent
pub fn new(id: u32) -> Option<IrqEvent>
pub fn for_line(line: &hardroxide_irq::IrqLine) -> Option<IrqEvent>
pub fn wait(&self, timeout: u64) -> bool
pub fn rearm(&self)
pub fn id(&self) -> u32
```

`IrqEvent` binds a GIC interrupt to a task: the task sleeps in `wait` and the kernel's interrupt path wakes it, with no handler to write
(a more urgent task runs as soon as the interrupt returns). When the interrupt fires the line is **masked in the GIC**, so a level-triggered source
cannot storm; the task clears the interrupt in the device (the GPIO's flag, the interrupt controller's acknowledge) and then calls `rearm`, which unmasks the
line again (and is also the call that first enables it, after the GIC and the device are set up). `for_line` takes a generated `IrqLine` that reaches the PS directly
(for example `pac::system_interrupts::IRQ_IRQ`, the output of an AXI interrupt controller); `new(id)` takes any GIC interrupt ID. At most eight events exist;
an event that fires while the task is not waiting is remembered once. Dropping the `IrqEvent` frees its slot. On the PYNQ-Z2 a
simulated interrupt at the AXI interrupt controller reached a priority-6 task in 4.2 us on average and 6.2 us at worst over 2000 events with a
processor hog and a cache-thrashing task running ([Verification gaps](../08-verification-gaps.md)).

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

## Measured

Two hours on a PYNQ-Z2 under load (a never-sleeping low-priority task and a cache-thrashing task): a 10 ms periodic task was released
at most 10.2 us late (mean 1.06 us) over 726,197 releases with none missed, and a software interrupt reached its waiting
task within 5.4 us (mean 3.95 us). Details and limits: [Verification gaps](../08-verification-gaps.md); the program is
`baremetal/rt-jitter`.

On an RFSoC 4x2 (Cortex-A53, two hours, same program and load): release lateness at most 5.9 us (mean 0.77 us) over 720,223
releases with none missed; interrupt to task at most 4.7 us (mean 3.04 us).

## Statistics

```rust,ignore
pub enum TaskState
pub struct TaskInfo
pub fn task_slots() -> usize
pub fn task_info(slot: usize) -> Option<TaskInfo>
pub fn irq_ticks() -> u64
pub fn cpu_total_ticks() -> u64
```

For a monitor page or a console report. `task_info(slot)` for each slot in `0..task_slots()` (slot 0 is the idle task; `None` for a free slot) gives a task's `name`, `priority` (the effective one, raised by priority inheritance),
`state` (`Running`, `Ready`, `Delayed` or `Blocked`), `run_ticks`, `stack_size` and `stack_used`. **`run_ticks` is processor time in timer ticks (the peripheral clock, 325 MHz on the PYNQ-Z2), charged at every context switch from a timestamp of the
global timer**, not sampled at the kernel tick: a sampling counter was tried first and was wrong, because a task released by the tick is not yet running when the tick samples, so a 10 ms control task showed 0% and the
processor hog 99%. The time spent in **interrupt handlers and the kernel's interrupt path** is counted separately (`irq_ticks`) and is not charged to the task that was interrupted, so a 97 kHz audio interrupt shows as 10% of the processor on
its own line. A task's share is `run_ticks / cpu_total_ticks()` (the sum of every task's time and the interrupt time). `stack_used` is a high-water mark: stacks start zero-filled and the mark is the first byte from the far end that has been written,
so it is an estimate (a task that wrote zeros there is not seen) and costs a scan of the stack, which `task_info` does **with interrupts on** (it first copies the task's fields inside the critical section; scanning a 256 KiB stack with
interrupts masked delayed the control loop by 409 us the first time it was written). Measured on a PYNQ-Z2 with an audio interrupt at 97.656 kHz, a 720p display, a TLS web server and a processor hog together: the 10 ms control task's worst deviation stayed at 4 us ([Verification gaps](../08-verification-gaps.md)).

## Not yet

Event flags and task notifications, software timers, task suspend/delete from other tasks, GIC-priority-mask critical
sections (so urgent interrupts are never delayed by the kernel), and a tickless idle.
