//! The kernel API: tasks, time, semaphores, mutexes and queues. Arm only.
//!
//! Single core. The scheduler state is only touched with interrupts masked (a critical
//! section), which on one core is mutual exclusion. A blocking call marks the task blocked
//! inside a critical section and then executes a supervisor call; the SVC entry switches to
//! whichever task the scheduler picks. Interrupt handlers may give semaphores and send to
//! queues (`*_from_isr`), which never switch tasks themselves: the switch happens as the
//! interrupt returns, if a more urgent task became ready.

use crate::port::{self, platform};
use crate::sched::{FOREVER, ObjId, Sched, TaskId};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use hardroxide_mmio::{MmioBackend, PhysMem};
use platform::{Gic, irq, map};

static mut SCHED: Sched = Sched::new();
/// The time stamp (low word of a free-running timer) when processor time was last charged, and the time spent in interrupt
/// handlers, in that timer's ticks.
static LAST_STAMP: AtomicU32 = AtomicU32::new(0);
static IRQ_TICKS: AtomicU64 = AtomicU64::new(0);

/// Charge the processor time since the last charge to the running task.
fn charge_until(s: &mut Sched, now: u32) {
    let dt = now.wrapping_sub(LAST_STAMP.load(Ordering::Relaxed));
    s.charge(u64::from(dt));
    LAST_STAMP.store(now, Ordering::Relaxed);
}
static TICK_HZ: AtomicU32 = AtomicU32::new(0);
static STARTED: AtomicU32 = AtomicU32::new(0);

/// Whether [`start`] has run.
fn started() -> bool {
    STARTED.load(Ordering::Acquire) != 0
}

/// Run `f` on the scheduler state with interrupts masked.
#[inline(always)]
fn cs<R>(f: impl FnOnce(&mut Sched) -> R) -> R {
    platform::without_irq(|| {
        // SAFETY: single core, interrupts masked: nothing else can touch `SCHED` now.
        f(unsafe { &mut *core::ptr::addr_of_mut!(SCHED) })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The task table or the synchronisation-object table is full, or the priority is 0.
    NoSlot,
    /// The stack is too small for a context frame plus work.
    StackTooSmall,
}

/// Convert milliseconds to ticks (rounded up, at least 1 for a non-zero time).
pub fn ms_to_ticks(ms: u32) -> u64 {
    let hz = u64::from(TICK_HZ.load(Ordering::Relaxed).max(1));
    (u64::from(ms) * hz).div_ceil(1000)
}

/// Ticks since the kernel started.
pub fn ticks() -> u64 {
    cs(|s| s.tick)
}

/// Define a stack for a task: `static` memory handed out once.
///
/// ```ignore
/// let stack = hardroxide_rt::task_stack!(4096).unwrap();
/// ```
#[macro_export]
macro_rules! task_stack {
    ($size:expr) => {{
        #[repr(align(8))]
        #[allow(dead_code)]
        struct Stack([u8; $size]);
        static mut STACK: Stack = Stack([0; $size]);
        static TAKEN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
        if TAKEN.swap(true, core::sync::atomic::Ordering::AcqRel) {
            None
        } else {
            let len: usize = $size;
            // SAFETY: `TAKEN` guarantees this is the only reference ever made to `STACK`.
            Some(unsafe {
                core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STACK) as *mut u8, len)
            })
        }
    }};
}

extern "C" fn task_returned() -> ! {
    cs(|s| s.exit_current());
    loop {
        port::yield_now();
    }
}

fn idle() {
    loop {
        platform::wfi();
    }
}

/// Create a task with priority `prio` (1 and up; **higher is more urgent**) that runs
/// `entry` on `stack` (at least 512 bytes on the Zynq-7000, 2048 on UltraScale+, where a saved context alone is 800
/// bytes). May be called before [`start`] or from a task.
pub fn spawn(
    name: &'static str,
    prio: u8,
    stack: &'static mut [u8],
    entry: fn(),
) -> Result<TaskId, Error> {
    if stack.len() < platform::MIN_STACK {
        return Err(Error::StackTooSmall);
    }
    let (base, len) = (stack.as_ptr() as usize, stack.len());
    let sp = port::init_frame(stack, entry, task_returned);
    cs(|s| {
        let id = s.add_task(name, prio, sp)?;
        s.set_stack(id, base, len);
        Some(id)
    })
    .ok_or(Error::NoSlot)
}

/// What a task is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Running now (the task that asked, if it asks about itself).
    Running,
    /// Ready to run, waiting for a more urgent task to finish or sleep.
    Ready,
    /// Sleeping for a time.
    Delayed,
    /// Waiting for a semaphore, a mutex, a queue or an interrupt.
    Blocked,
}

/// A snapshot of one task, for a monitor.
#[derive(Debug, Clone, Copy)]
pub struct TaskInfo {
    /// The task's slot (0 is the idle task).
    pub slot: usize,
    pub name: &'static str,
    /// The effective priority (raised while it holds a mutex others want).
    pub priority: u8,
    pub state: TaskState,
    /// Processor time this task has used, in timer ticks (on the Zynq-7000 the peripheral clock, `clocks::periph_hz`, 325 MHz on
    /// the PYNQ-Z2; on UltraScale+ the system counter, `clocks::timestamp_hz`, 33 MHz on the RFSoC 4x2),
    /// charged at every context switch. Interrupt handlers' time is counted separately ([`irq_ticks`]), not charged to the task they interrupted.
    /// A task's share of the processor is `run_ticks / cpu_total_ticks()`.
    pub run_ticks: u64,
    pub stack_size: usize,
    /// How much of the stack has been used at most: the stack is zero-filled and the mark is the first byte from the far end that
    /// has been written, so it is an estimate (a task that wrote zeros there is not seen).
    pub stack_used: usize,
}

/// Processor time spent in interrupt handlers and the kernel's interrupt path since the kernel started, in timer ticks.
pub fn irq_ticks() -> u64 {
    IRQ_TICKS.load(Ordering::Relaxed)
}

/// All the processor time accounted for: every task's run time plus the interrupt time (the sum to divide a share by).
pub fn cpu_total_ticks() -> u64 {
    let tasks: u64 = cs(|s| s.tasks.iter().map(|t| t.ticks_run).sum());
    tasks + irq_ticks()
}

/// The slots a task can occupy (`0..task_slots()`; slot 0 is the idle task).
pub fn task_slots() -> usize {
    crate::sched::MAX_TASKS
}

/// A snapshot of the task in `slot`, or `None` if the slot is free.
///
/// The task's fields are copied with interrupts masked, but the stack scan (which can take hundreds of microseconds for a big stack) runs with
/// interrupts on, so asking for statistics never delays an interrupt or a more urgent task.
pub fn task_info(slot: usize) -> Option<TaskInfo> {
    let (name, priority, state, run_ticks, base, len) = cs(|s| {
        let t = s.tasks.get(slot)?;
        let state = match t.state {
            crate::sched::State::Free => return None,
            _ if slot == s.current => TaskState::Running,
            crate::sched::State::Ready => TaskState::Ready,
            crate::sched::State::Delayed => TaskState::Delayed,
            crate::sched::State::Blocked => TaskState::Blocked,
        };
        Some((
            t.name,
            t.prio,
            state,
            t.ticks_run,
            t.stack_base,
            t.stack_len,
        ))
    })?;
    let stack_used = if len == 0 {
        0
    } else {
        // SAFETY: the stack of a live task is a `'static` byte array that this scan only reads (a task running meanwhile may write it; the answer is a snapshot).
        let bytes = unsafe { core::slice::from_raw_parts(base as *const u8, len) };
        len - bytes.iter().take_while(|&&b| b == 0).count()
    };
    Some(TaskInfo {
        slot,
        name,
        priority,
        state,
        run_ticks,
        stack_size: len,
        stack_used,
    })
}

extern "C" fn dummy() -> ! {
    loop {
        platform::wfe();
    }
}

/// Start the scheduler: create the idle task, start the tick (a timer interrupt at `tick_hz`) and run the most urgent task.
/// Never returns. The GIC must already be initialised (`Design::init` does it). `periph_hz` is the clock of the tick's timer:
/// on the Zynq-7000 the private timer's (`clocks::periph_hz`), on UltraScale+ the system counter's
/// (`clocks::timestamp_hz`).
pub fn start(periph_hz: u64, tick_hz: u32) -> ! {
    TICK_HZ.store(tick_hz, Ordering::Relaxed);
    let idle_stack = crate::task_stack!(platform::IDLE_STACK).expect("kernel started twice");
    let (idle_base, idle_len) = (idle_stack.as_ptr() as usize, idle_stack.len());
    let idle_sp = port::init_frame(idle_stack, idle, dummy);
    cs(|s| {
        s.init_idle(idle_sp);
        s.set_stack(0, idle_base, idle_len);
    });
    // SAFETY: the GIC and tick timer belong to the kernel from here on.
    let be = unsafe { PhysMem::new() };
    let gic = Gic::new(
        be.map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE)
            .expect("gic"),
        be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE)
            .expect("gic"),
    );
    gic.set_priority(platform::TICK_INTERRUPT, 0x40)
        .expect("gic");
    gic.enable(platform::TICK_INTERRUPT).expect("gic");
    platform::start_tick(&be, periph_hz, tick_hz);
    LAST_STAMP.store(platform::stamp(), Ordering::Relaxed);
    STARTED.store(1, Ordering::Release);
    let first = cs(|s| {
        s.schedule();
        s.tasks[s.current].sp
    });
    // SAFETY: `first` is a frame built by `init_frame`; this never returns.
    unsafe { port::hrx_rt_start_first(first) }
}

/// Save `sp` for the running task, choose the next one, and return its stack pointer. Before [`start`] there is no
/// running task: an interrupt then returns to where it came from.
fn switch(sp: usize, force: bool) -> usize {
    if !started() {
        return sp;
    }
    cs(|s| {
        s.tasks[s.current].sp = sp;
        if force || s.should_preempt() {
            s.schedule();
        }
        s.tasks[s.current].sp
    })
}

#[unsafe(no_mangle)]
extern "C" fn hrx_rt_svc(sp: usize) -> usize {
    // Before `start` there is nothing to switch to: a call from `main` returns to it.
    if !started() {
        return sp;
    }
    // A blocking call or an explicit yield: always reschedule.
    let now = platform::stamp();
    cs(|s| {
        charge_until(s, now);
        s.tasks[s.current].sp = sp;
        s.schedule();
        s.tasks[s.current].sp
    })
}

#[unsafe(no_mangle)]
extern "C" fn hrx_rt_irq(sp: usize) -> usize {
    let t_in = platform::stamp();
    // SAFETY: the GIC and tick timer are used only by the interrupt path.
    let be = unsafe { PhysMem::new() };
    let (Ok(d), Ok(c)) = (
        be.map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE),
        be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE),
    ) else {
        return sp;
    };
    let gic = Gic::new(d, c);
    let mut tick_switch = false;
    if let Ok(Some(id)) = gic.acknowledge() {
        if id == platform::TICK_INTERRUPT {
            platform::acknowledge_tick(&be);
            tick_switch = cs(|s| s.tick());
        } else {
            match irq::dispatch(id) {
                irq::Dispatched::Handler => {}
                irq::Dispatched::Event => {
                    irq_event_fired(id);
                    let _ = gic.disable(id);
                }
                irq::Dispatched::Unhandled => {
                    let _ = gic.disable(id);
                }
            }
        }
        let _ = gic.end_of_interrupt(id);
    }
    // The running task is charged up to the moment the interrupt came, and the interrupt's own time goes to its own account.
    let t_out = platform::stamp();
    cs(|s| charge_until(s, t_in));
    IRQ_TICKS.fetch_add(u64::from(t_out.wrapping_sub(t_in)), Ordering::Relaxed);
    LAST_STAMP.store(t_out, Ordering::Relaxed);
    switch(sp, tick_switch)
}

const IRQ_EVENTS: usize = 8;
const FREE: u32 = u32::MAX;
static EVENT_IDS: [AtomicU32; IRQ_EVENTS] = [const { AtomicU32::new(FREE) }; IRQ_EVENTS];
static EVENT_SEMS: [Semaphore; IRQ_EVENTS] = [const { Semaphore::new(0, 1) }; IRQ_EVENTS];

/// Called from the IRQ path when an interrupt in event mode fired: wake whoever waits on it.
fn irq_event_fired(id: u32) {
    for (slot, owner) in EVENT_IDS.iter().enumerate() {
        if owner.load(Ordering::Acquire) == id {
            EVENT_SEMS[slot].give_from_isr();
        }
    }
}

/// A hardware interrupt a task can sleep on: the kernel's version of waiting for a PL
/// interrupt. [`IrqEvent::new`] puts a GIC interrupt in event mode; when it fires, the interrupt
/// path masks it in the GIC and wakes the task blocked in [`IrqEvent::wait`] (a more urgent
/// task runs as soon as the interrupt returns). The task then clears the interrupt's source in
/// the device and calls [`IrqEvent::rearm`] to unmask the line for the next one. At most eight
/// events exist; an event that fires while nobody waits is remembered (once).
#[derive(Debug)]
pub struct IrqEvent {
    slot: usize,
    id: u32,
}

impl IrqEvent {
    /// Bind GIC interrupt `id` (for a PL interrupt see `ps7::map::f2p_irq` or `psu::map::pl_irq`). `None` if `id` is
    /// out of range or all eight events are in use. The line stays masked until [`IrqEvent::rearm`].
    pub fn new(id: u32) -> Option<IrqEvent> {
        if !irq::register_event(id) {
            return None;
        }
        for (slot, owner) in EVENT_IDS.iter().enumerate() {
            if owner
                .compare_exchange(FREE, id, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(IrqEvent { slot, id });
            }
        }
        None
    }

    /// Bind a generated interrupt line. `None` if it does not reach the PS directly (it feeds an
    /// interrupt controller, or could not be traced), or as for [`IrqEvent::new`].
    pub fn for_line(line: &hardroxide_irq::IrqLine) -> Option<IrqEvent> {
        IrqEvent::new(map::SPI_BASE + line.gic_spi?)
    }

    /// Sleep until the interrupt fires, for at most `timeout` ticks ([`WAIT_FOREVER`] for ever, 0
    /// to poll). `true` if it fired.
    pub fn wait(&self, timeout: u64) -> bool {
        EVENT_SEMS[self.slot].take(timeout)
    }

    /// Unmask the interrupt in the GIC. Call it after the device's own interrupt flag has been
    /// cleared, or a level-triggered line fires again at once. Also the call that first enables it.
    pub fn rearm(&self) {
        // SAFETY: the GIC windows are only used by the interrupt layer.
        let be = unsafe { PhysMem::new() };
        if let (Ok(d), Ok(c)) = (
            be.map(map::GIC_DIST_BASE as u64, map::GIC_DIST_SIZE),
            be.map(map::GIC_CPU_BASE as u64, map::GIC_CPU_SIZE),
        ) {
            let _ = Gic::new(d, c).enable(self.id);
        }
    }

    /// The GIC interrupt ID.
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl Drop for IrqEvent {
    fn drop(&mut self) {
        EVENT_IDS[self.slot].store(FREE, Ordering::Release);
        irq::unregister(self.id);
    }
}

/// Give up the processor to an equally urgent ready task (or keep it if there is none).
pub fn yield_now() {
    // Re-queue the caller behind its equals by marking nothing: the scheduler's round-robin
    // scan starts after the current task.
    port::yield_now();
}

/// Sleep for at least `ticks` ticks. Before [`start`] it returns at once.
pub fn delay(ticks: u64) {
    if !started() {
        return;
    }
    cs(|s| s.delay_current(ticks));
    port::yield_now();
}

/// Sleep for at least `ms` milliseconds.
pub fn delay_ms(ms: u32) {
    delay(ms_to_ticks(ms));
}

/// A periodic release: construct it once at the top of a task's loop, then call
/// [`wait_next`](Self::wait_next) at the end of each iteration. The period does not drift
/// however long an iteration takes.
pub struct Periodic {
    last: u64,
    period: u64,
}

impl Periodic {
    pub fn new(period_ticks: u64) -> Self {
        Periodic {
            last: ticks(),
            period: period_ticks.max(1),
        }
    }
    pub fn every_ms(ms: u32) -> Self {
        Self::new(ms_to_ticks(ms))
    }
    /// Sleep until the next release. Returns `false` (without sleeping) if that release is
    /// already in the past: the iteration overran its period.
    pub fn wait_next(&mut self) -> bool {
        let slept = cs(|s| s.delay_until_current(&mut self.last, self.period));
        if slept {
            port::yield_now();
        }
        slept
    }
}

/// Lazily allocated kernel object id (so objects can be `static`).
struct LazyId(AtomicUsize);

impl LazyId {
    const fn new() -> Self {
        LazyId(AtomicUsize::new(usize::MAX))
    }
    fn get(&self, alloc: impl FnOnce(&mut Sched) -> Option<ObjId>) -> ObjId {
        let v = self.0.load(Ordering::Acquire);
        if v != usize::MAX {
            return v;
        }
        cs(|s| {
            let v = self.0.load(Ordering::Acquire);
            if v != usize::MAX {
                return v;
            }
            let id = alloc(s).expect("kernel object table full");
            self.0.store(id, Ordering::Release);
            id
        })
    }
}

/// A counting semaphore.
pub struct Semaphore {
    id: LazyId,
    initial: u32,
    max: u32,
}

impl Semaphore {
    /// A semaphore with `initial` permits, holding at most `max`. `const`, so it can be a `static`.
    pub const fn new(initial: u32, max: u32) -> Self {
        Semaphore {
            id: LazyId::new(),
            initial,
            max,
        }
    }

    fn id(&self) -> ObjId {
        self.id.get(|s| s.new_semaphore(self.initial, self.max))
    }

    /// Take a permit, waiting up to `timeout` ticks ([`FOREVER`] to wait for ever, 0 to
    /// poll). `true` if a permit was taken. Before [`start`] it never waits.
    pub fn take(&self, timeout: u64) -> bool {
        let id = self.id();
        let blocked = cs(|s| {
            if s.sem_try_take(id) {
                return Some(true);
            }
            if timeout == 0 || !started() {
                return Some(false);
            }
            s.block_current_on(id, timeout);
            None
        });
        if let Some(r) = blocked {
            return r;
        }
        port::yield_now();
        // Resumed: a permit was handed to us, unless the wait timed out.
        cs(|s| !s.tasks[s.current].timed_out)
    }

    /// Give a permit, from a task. Switches at once if that wakes a more urgent task.
    pub fn give(&self) {
        let id = self.id();
        if cs(|s| {
            s.sem_give(id);
            s.should_preempt()
        }) {
            port::yield_now();
        }
    }

    /// Give a permit from an interrupt handler. Never switches; the switch, if needed, happens
    /// when the interrupt returns.
    pub fn give_from_isr(&self) {
        let id = self.id();
        cs(|s| {
            s.sem_give(id);
        });
    }
}

/// A mutual-exclusion lock with priority inheritance: while a more urgent task waits for it,
/// the holder runs at that task's priority, so a medium-priority task cannot starve it.
pub struct Mutex {
    id: LazyId,
}

/// Held while the lock is. Unlocks on drop.
pub struct MutexGuard<'a>(&'a Mutex);

impl Mutex {
    pub const fn new() -> Self {
        Mutex { id: LazyId::new() }
    }

    fn id(&self) -> ObjId {
        self.id.get(|s| s.new_mutex())
    }

    /// Take the lock, waiting up to `timeout` ticks. `None` on timeout. Before [`start`] it never waits.
    pub fn lock(&self, timeout: u64) -> Option<MutexGuard<'_>> {
        let id = self.id();
        let outcome = cs(|s| {
            if s.mutex_try_take(id) {
                return Some(true);
            }
            if timeout == 0 || !started() {
                return Some(false);
            }
            s.mutex_boost_owner(id);
            s.block_current_on(id, timeout);
            None
        });
        match outcome {
            Some(true) => Some(MutexGuard(self)),
            Some(false) => None,
            None => {
                port::yield_now();
                // Resumed: ownership was handed to us, unless the wait timed out.
                if cs(|s| s.tasks[s.current].timed_out) {
                    None
                } else {
                    Some(MutexGuard(self))
                }
            }
        }
    }
}

impl Default for Mutex {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MutexGuard<'_> {
    fn drop(&mut self) {
        let id = self.0.id();
        if cs(|s| {
            let me = s.current;
            let _ = s.mutex_give(id, me);
            s.should_preempt()
        }) {
            port::yield_now();
        }
    }
}

/// A bounded first-in-first-out queue of `Copy` items between tasks, and from interrupt
/// handlers to tasks.
pub struct Queue<T: Copy, const N: usize> {
    items: Semaphore,
    space: Semaphore,
    buf: UnsafeCell<[MaybeUninit<T>; N]>,
    head: UnsafeCell<usize>,
    tail: UnsafeCell<usize>,
}

// SAFETY: all access to the buffer and indices is inside critical sections (or follows a
// semaphore permit that grants exclusive use of that slot), so sharing between tasks and
// interrupt handlers on one core is sound.
unsafe impl<T: Copy + Send, const N: usize> Sync for Queue<T, N> {}

impl<T: Copy, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        Queue {
            items: Semaphore::new(0, N as u32),
            space: Semaphore::new(N as u32, N as u32),
            buf: UnsafeCell::new([const { MaybeUninit::uninit() }; N]),
            head: UnsafeCell::new(0),
            tail: UnsafeCell::new(0),
        }
    }

    fn push(&self, v: T) {
        cs(|_| {
            // SAFETY: inside a critical section; a `space` permit guarantees a free slot.
            unsafe {
                let t = *self.tail.get();
                (*self.buf.get())[t].write(v);
                *self.tail.get() = (t + 1) % N;
            }
        });
    }

    fn pop(&self) -> T {
        cs(|_| {
            // SAFETY: inside a critical section; an `items` permit guarantees a stored item.
            unsafe {
                let h = *self.head.get();
                let v = (*self.buf.get())[h].assume_init();
                *self.head.get() = (h + 1) % N;
                v
            }
        })
    }

    /// Append `v`, waiting up to `timeout` ticks for room. `false` if there was none in time.
    pub fn send(&self, v: T, timeout: u64) -> bool {
        if !self.space.take(timeout) {
            return false;
        }
        self.push(v);
        self.items.give();
        true
    }

    /// Take the oldest item, waiting up to `timeout` ticks for one.
    pub fn recv(&self, timeout: u64) -> Option<T> {
        if !self.items.take(timeout) {
            return None;
        }
        let v = self.pop();
        self.space.give();
        Some(v)
    }

    /// Append from an interrupt handler without waiting; `false` if the queue is full.
    pub fn send_from_isr(&self, v: T) -> bool {
        if !self.space.take(0) {
            return false;
        }
        self.push(v);
        self.items.give_from_isr();
        true
    }
}

impl<T: Copy, const N: usize> Default for Queue<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// A value shared between tasks behind a [`Mutex`] (with priority inheritance): the way to
/// let several tasks use one generated `Design`, or any other resource.
///
/// ```ignore
/// static DESIGN: Shared<Design<PhysMem>> = Shared::new();
/// // in main, before `start`:
/// DESIGN.set(design);
/// // in a task:
/// let d = DESIGN.lock(WAIT_FOREVER).unwrap();   // &mut Design while the guard lives
/// ```
pub struct Shared<T> {
    lock: Mutex,
    value: UnsafeCell<Option<T>>,
}

// SAFETY: the value is only reachable through `lock`, which grants one task at a time (or, in
// `set`, runs before the kernel starts with interrupts masked).
unsafe impl<T: Send> Sync for Shared<T> {}

pub struct SharedGuard<'a, T> {
    shared: &'a Shared<T>,
    _guard: MutexGuard<'a>,
}

impl<T> Shared<T> {
    pub const fn new() -> Self {
        Shared {
            lock: Mutex::new(),
            value: UnsafeCell::new(None),
        }
    }

    /// Store the value. Call this before [`start`] (it does not take the lock).
    pub fn set(&self, v: T) {
        platform::without_irq(|| {
            // SAFETY: before the kernel runs nothing else can access the value.
            unsafe { *self.value.get() = Some(v) };
        });
    }

    /// Lock and borrow the value, waiting up to `timeout` ticks. `None` on timeout or if
    /// [`set`](Self::set) was never called.
    pub fn lock(&self, timeout: u64) -> Option<SharedGuard<'_, T>> {
        let g = self.lock.lock(timeout)?;
        // SAFETY: the lock is held.
        if unsafe { (*self.value.get()).is_none() } {
            return None;
        }
        Some(SharedGuard {
            shared: self,
            _guard: g,
        })
    }
}

impl<T> Default for Shared<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> core::ops::Deref for SharedGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the lock is held and the value is `Some` (checked in `lock`).
        unsafe { (*self.shared.value.get()).as_ref().unwrap_unchecked() }
    }
}

impl<T> core::ops::DerefMut for SharedGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as in `deref`, and the guard is the only borrower.
        unsafe { (*self.shared.value.get()).as_mut().unwrap_unchecked() }
    }
}

/// Wait for ever.
pub const WAIT_FOREVER: u64 = FOREVER;
