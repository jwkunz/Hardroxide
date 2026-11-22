//! The scheduling policy: which task runs, who is waiting for what, and for how long.
//!
//! Pure data and logic with no hardware access, so it is unit-tested on the host. The
//! Cortex-A9 port ([`crate::port`]) switches stacks; the kernel ([`crate::kernel`]) wraps
//! these operations in critical sections and yields when they say another task should run.
//!
//! Priorities: **a higher number is more urgent**. Task 0 is the idle task (priority 0) and
//! is always ready. Among tasks of equal priority the processor is shared round-robin, one
//! tick each.

pub const MAX_TASKS: usize = 16;
pub const MAX_OBJS: usize = 32;
pub type TaskId = usize;
pub type ObjId = usize;
/// The idle task: priority 0, always ready.
pub const IDLE: TaskId = 0;
/// No timeout.
pub const FOREVER: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Free,
    Ready,
    Delayed,
    Blocked,
}

#[derive(Clone, Copy, Debug)]
pub struct Tcb {
    pub state: State,
    /// Effective priority (raised by priority inheritance).
    pub prio: u8,
    pub base_prio: u8,
    /// Saved stack pointer while not running (managed by the port).
    pub sp: usize,
    /// Tick at which a delay or a timed wait ends.
    pub wake: u64,
    pub waiting: Option<ObjId>,
    /// The last wait ended because its time ran out.
    pub timed_out: bool,
    seq: u64,
    pub name: &'static str,
    /// Time this task has run, in timer ticks (charged at every context switch).
    pub ticks_run: u64,
    /// Where the task's stack is (for the high-water mark); zero for the idle task's slot until set.
    pub stack_base: usize,
    pub stack_len: usize,
}

const FREE_TCB: Tcb = Tcb {
    state: State::Free,
    prio: 0,
    base_prio: 0,
    sp: 0,
    wake: 0,
    waiting: None,
    timed_out: false,
    seq: 0,
    name: "",
    ticks_run: 0,
    stack_base: 0,
    stack_len: 0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Free,
    Semaphore,
    Mutex,
}

#[derive(Clone, Copy, Debug)]
pub struct Obj {
    pub kind: Kind,
    pub count: u32,
    pub max: u32,
    pub owner: Option<TaskId>,
}

const FREE_OBJ: Obj = Obj {
    kind: Kind::Free,
    count: 0,
    max: 0,
    owner: None,
};

/// The task releasing a mutex is not its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotOwner;

pub struct Sched {
    pub tasks: [Tcb; MAX_TASKS],
    pub objs: [Obj; MAX_OBJS],
    pub current: TaskId,
    pub tick: u64,
    seq: u64,
}

impl Default for Sched {
    fn default() -> Self {
        Self::new()
    }
}

impl Sched {
    pub const fn new() -> Self {
        Sched {
            tasks: [FREE_TCB; MAX_TASKS],
            objs: [FREE_OBJ; MAX_OBJS],
            current: IDLE,
            tick: 0,
            seq: 0,
        }
    }

    /// Create the idle task in slot 0 (priority 0, ready).
    pub fn init_idle(&mut self, sp: usize) {
        self.tasks[IDLE] = Tcb {
            state: State::Ready,
            sp,
            name: "idle",
            ..FREE_TCB
        };
        self.current = IDLE;
    }

    /// Add a ready task with priority `prio` (1 and up). `None` if the table is full or
    /// `prio` is 0.
    pub fn add_task(&mut self, name: &'static str, prio: u8, sp: usize) -> Option<TaskId> {
        if prio == 0 {
            return None;
        }
        let id = (1..MAX_TASKS).find(|&i| self.tasks[i].state == State::Free)?;
        self.tasks[id] = Tcb {
            state: State::Ready,
            prio,
            base_prio: prio,
            sp,
            name,
            ..FREE_TCB
        };
        Some(id)
    }

    /// Charge `ticks` of processor time to the running task.
    pub fn charge(&mut self, ticks: u64) {
        self.tasks[self.current].ticks_run += ticks;
    }

    /// Record where task `id`'s stack is.
    pub fn set_stack(&mut self, id: TaskId, base: usize, len: usize) {
        self.tasks[id].stack_base = base;
        self.tasks[id].stack_len = len;
    }

    /// Mark the current task finished and free its slot.
    pub fn exit_current(&mut self) {
        let me = self.current;
        if me != IDLE {
            self.tasks[me] = FREE_TCB;
        }
    }

    /// The task that should run now: the most urgent ready one, sharing the processor
    /// round-robin with equals (the search starts after the current task).
    pub fn pick_next(&self) -> TaskId {
        let mut best = IDLE;
        let mut best_prio = 0u8;
        for off in 1..=MAX_TASKS {
            let i = (self.current + off) % MAX_TASKS;
            let t = &self.tasks[i];
            if t.state == State::Ready && t.prio > best_prio {
                best = i;
                best_prio = t.prio;
            }
        }
        // The current task keeps the processor unless someone more urgent, or an equal that
        // the scan found first (round-robin), is ready. It is last in scan order, so an equal
        // wins only if it came first.
        let cur = &self.tasks[self.current];
        if cur.state == State::Ready && cur.prio >= best_prio && best_prio < cur.prio {
            return self.current;
        }
        if best_prio == 0 && cur.state == State::Ready {
            return self.current;
        }
        best
    }

    /// Choose the next task and make it current; returns it.
    pub fn schedule(&mut self) -> TaskId {
        self.current = self.pick_next();
        self.current
    }

    /// Advance time by one tick: wake delayed tasks and time out waits. Returns `true` if
    /// the running task should be switched out (a more urgent task became ready, or an
    /// equal one is waiting for its turn).
    pub fn tick(&mut self) -> bool {
        self.tick += 1;
        let now = self.tick;
        for t in self.tasks.iter_mut() {
            match t.state {
                State::Delayed if t.wake <= now => t.state = State::Ready,
                State::Blocked if t.wake <= now => {
                    t.state = State::Ready;
                    t.waiting = None;
                    t.timed_out = true;
                }
                _ => {}
            }
        }
        self.pick_next() != self.current
    }

    /// Delay the current task for `ticks` ticks (it becomes ready again when they are over).
    pub fn delay_current(&mut self, ticks: u64) {
        let me = self.current;
        self.tasks[me].state = State::Delayed;
        self.tasks[me].wake = self.tick.saturating_add(ticks.max(1));
    }

    /// Periodic release: delay until `*last_wake + period`, and advance `*last_wake` by
    /// exactly one period (so the rate does not drift). Returns `false` and does not delay if
    /// that time has already passed (a deadline miss).
    pub fn delay_until_current(&mut self, last_wake: &mut u64, period: u64) -> bool {
        let next = last_wake.saturating_add(period);
        *last_wake = next;
        if next <= self.tick {
            return false;
        }
        let me = self.current;
        self.tasks[me].state = State::Delayed;
        self.tasks[me].wake = next;
        true
    }

    fn alloc_obj(&mut self, o: Obj) -> Option<ObjId> {
        let id = self.objs.iter().position(|x| x.kind == Kind::Free)?;
        self.objs[id] = o;
        Some(id)
    }

    /// A counting semaphore with `initial` permits, at most `max`.
    pub fn new_semaphore(&mut self, initial: u32, max: u32) -> Option<ObjId> {
        self.alloc_obj(Obj {
            kind: Kind::Semaphore,
            count: initial.min(max),
            max,
            owner: None,
        })
    }

    pub fn new_mutex(&mut self) -> Option<ObjId> {
        self.alloc_obj(Obj {
            kind: Kind::Mutex,
            count: 0,
            max: 1,
            owner: None,
        })
    }

    /// Block the current task on `obj`, for at most `timeout` ticks.
    pub fn block_current_on(&mut self, obj: ObjId, timeout: u64) {
        let me = self.current;
        self.seq += 1;
        let t = &mut self.tasks[me];
        t.state = State::Blocked;
        t.waiting = Some(obj);
        t.timed_out = false;
        t.seq = self.seq;
        t.wake = if timeout == FOREVER {
            FOREVER
        } else {
            self.tick.saturating_add(timeout)
        };
    }

    /// The most urgent waiter on `obj` (the longest-waiting among equals), made ready.
    fn wake_waiter(&mut self, obj: ObjId) -> Option<TaskId> {
        let id = (0..MAX_TASKS)
            .filter(|&i| {
                self.tasks[i].state == State::Blocked && self.tasks[i].waiting == Some(obj)
            })
            .min_by_key(|&i| (core::cmp::Reverse(self.tasks[i].prio), self.tasks[i].seq))?;
        let t = &mut self.tasks[id];
        t.state = State::Ready;
        t.waiting = None;
        t.timed_out = false;
        Some(id)
    }

    /// Take a permit if there is one.
    pub fn sem_try_take(&mut self, obj: ObjId) -> bool {
        let o = &mut self.objs[obj];
        if o.count > 0 {
            o.count -= 1;
            true
        } else {
            false
        }
    }

    /// Give a permit. If a task is waiting it receives the permit directly. Returns the task
    /// woken, if any; the caller yields if that task is more urgent than the current one.
    pub fn sem_give(&mut self, obj: ObjId) -> Option<TaskId> {
        if let Some(t) = self.wake_waiter(obj) {
            return Some(t);
        }
        let o = &mut self.objs[obj];
        o.count = (o.count + 1).min(o.max);
        None
    }

    /// Whether a more urgent task than the current one is ready, or the current task can no
    /// longer run: switching is warranted now (not merely time-slicing among equals).
    pub fn should_preempt(&self) -> bool {
        let cur = &self.tasks[self.current];
        cur.state != State::Ready
            || self
                .tasks
                .iter()
                .any(|t| t.state == State::Ready && t.prio > cur.prio)
    }

    // ---- mutexes with priority inheritance ----

    /// Take the mutex for the current task if it is free (or already its own: not
    /// recursive, so that is refused).
    pub fn mutex_try_take(&mut self, obj: ObjId) -> bool {
        let me = self.current;
        let o = &mut self.objs[obj];
        if o.owner.is_none() {
            o.owner = Some(me);
            true
        } else {
            false
        }
    }

    /// The current task is about to block on the mutex: lend its priority to the owner (and
    /// along a chain of owners that are themselves blocked on mutexes).
    pub fn mutex_boost_owner(&mut self, obj: ObjId) {
        let prio = self.tasks[self.current].prio;
        let mut m = obj;
        for _ in 0..MAX_TASKS {
            let Some(owner) = self.objs[m].owner else {
                return;
            };
            if self.tasks[owner].prio >= prio {
                return;
            }
            self.tasks[owner].prio = prio;
            match self.tasks[owner].waiting {
                Some(next) if self.objs[next].kind == Kind::Mutex => m = next,
                _ => return,
            }
        }
    }

    /// The most urgent task waiting on any mutex `t` owns (0 if none).
    fn inherited_prio(&self, t: TaskId) -> u8 {
        let mut p = 0;
        for (id, o) in self.objs.iter().enumerate() {
            if o.kind == Kind::Mutex && o.owner == Some(t) {
                for w in self.tasks.iter() {
                    if w.state == State::Blocked && w.waiting == Some(id) {
                        p = p.max(w.prio);
                    }
                }
            }
        }
        p
    }

    /// Release the mutex held by `who`. Ownership passes to the most urgent waiter, if any
    /// (returned, to be made ready); `who`'s priority drops back to what it still deserves.
    /// `None` and no change if `who` does not own it.
    pub fn mutex_give(&mut self, obj: ObjId, who: TaskId) -> Result<Option<TaskId>, NotOwner> {
        if self.objs[obj].owner != Some(who) {
            return Err(NotOwner);
        }
        self.objs[obj].owner = None;
        let woken = self.wake_waiter(obj);
        if let Some(w) = woken {
            self.objs[obj].owner = Some(w);
            // The new owner inherits from whoever is still waiting behind it.
            let behind = self.inherited_prio(w);
            self.tasks[w].prio = self.tasks[w].base_prio.max(behind);
        }
        let keep = self.inherited_prio(who);
        self.tasks[who].prio = self.tasks[who].base_prio.max(keep);
        Ok(woken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> Sched {
        let mut s = Sched::new();
        s.init_idle(0);
        s
    }

    #[test]
    fn processor_time_is_charged_to_the_running_task() {
        let mut s = sched();
        let a = s.add_task("a", 5, 0).unwrap();
        let b = s.add_task("b", 3, 0).unwrap();
        s.schedule(); // a runs
        s.charge(30);
        s.delay_current(100); // a sleeps; b runs
        s.schedule();
        s.charge(70);
        s.delay_current(100); // b sleeps too: the idle task runs
        s.schedule();
        s.charge(5);
        assert_eq!(
            (
                s.tasks[a].ticks_run,
                s.tasks[b].ticks_run,
                s.tasks[IDLE].ticks_run
            ),
            (30, 70, 5)
        );
        s.set_stack(a, 0x1000, 512);
        assert_eq!((s.tasks[a].stack_base, s.tasks[a].stack_len), (0x1000, 512));
    }

    #[test]
    fn the_most_urgent_ready_task_runs() {
        let mut s = sched();
        let low = s.add_task("low", 1, 0).unwrap();
        let high = s.add_task("high", 5, 0).unwrap();
        let mid = s.add_task("mid", 3, 0).unwrap();
        assert_eq!(s.schedule(), high);
        s.delay_current(10); // high sleeps
        assert_eq!(s.schedule(), mid);
        s.delay_current(10);
        assert_eq!(s.schedule(), low);
        s.delay_current(10);
        assert_eq!(s.schedule(), IDLE);
    }

    #[test]
    fn equal_priorities_share_the_processor_round_robin() {
        let mut s = sched();
        let a = s.add_task("a", 2, 0).unwrap();
        let b = s.add_task("b", 2, 0).unwrap();
        let c = s.add_task("c", 2, 0).unwrap();
        assert_eq!(s.schedule(), a);
        let order: std::vec::Vec<_> = (0..6)
            .map(|_| {
                assert!(s.tick(), "a tick hands over to an equal");
                s.schedule()
            })
            .collect();
        assert_eq!(order, [b, c, a, b, c, a]);
    }

    #[test]
    fn preemption_is_for_more_urgent_tasks_only() {
        let mut s = sched();
        let a = s.add_task("a", 2, 0).unwrap();
        let b = s.add_task("b", 2, 0).unwrap();
        s.current = a;
        assert!(
            !s.should_preempt(),
            "an equal does not preempt (that is time slicing)"
        );
        let c = s.add_task("c", 3, 0).unwrap();
        assert!(s.should_preempt());
        s.tasks[c].state = State::Delayed;
        s.tasks[a].state = State::Blocked;
        assert!(s.should_preempt(), "the current task cannot run");
        let _ = b;
    }

    #[test]
    fn a_lone_task_is_not_switched_out_by_the_tick() {
        let mut s = sched();
        let a = s.add_task("a", 2, 0).unwrap();
        s.schedule();
        assert_eq!(s.current, a);
        for _ in 0..5 {
            assert!(!s.tick());
        }
    }

    #[test]
    fn delays_end_on_time_and_preempt_a_less_urgent_task() {
        let mut s = sched();
        let low = s.add_task("low", 1, 0).unwrap();
        let high = s.add_task("high", 4, 0).unwrap();
        s.schedule();
        assert_eq!(s.current, high);
        s.delay_current(3);
        assert_eq!(s.schedule(), low);
        assert!(!s.tick() && !s.tick());
        assert!(s.tick(), "tick 3: high is ready again and more urgent");
        assert_eq!(s.schedule(), high);
    }

    #[test]
    fn periodic_release_does_not_drift_and_reports_a_miss() {
        let mut s = sched();
        let t = s.add_task("t", 2, 0).unwrap();
        s.schedule();
        let mut last = s.tick; // 0
        for k in 1..=4u64 {
            assert!(s.delay_until_current(&mut last, 10));
            assert_eq!(s.tasks[t].wake, 10 * k);
            // The task is late waking by 3 ticks each time; the next release still lands on the grid.
            for _ in 0..(10 + 3) {
                s.tick();
            }
            s.schedule();
            assert_eq!(s.current, t);
            // Rewind the demonstration: pretend the work took long enough that the next release has passed.
            if k == 4 {
                s.tick = 100;
                assert!(
                    !s.delay_until_current(&mut last, 10),
                    "a missed release does not block"
                );
                assert_eq!(s.tasks[t].state, State::Ready);
            }
        }
    }

    #[test]
    fn a_semaphore_hands_its_permit_to_the_most_urgent_waiter() {
        let mut s = sched();
        let lo = s.add_task("lo", 2, 0).unwrap();
        let hi = s.add_task("hi", 5, 0).unwrap();
        let sem = s.new_semaphore(0, 1).unwrap();
        s.current = lo;
        s.block_current_on(sem, FOREVER);
        s.current = hi;
        s.block_current_on(sem, FOREVER);
        assert_eq!(s.sem_give(sem), Some(hi));
        assert_eq!(s.tasks[hi].state, State::Ready);
        assert_eq!(
            s.objs[sem].count, 0,
            "the permit went straight to the waiter"
        );
        assert_eq!(s.sem_give(sem), Some(lo));
        assert_eq!(s.sem_give(sem), None);
        assert_eq!(s.objs[sem].count, 1);
        assert!(s.sem_try_take(sem));
        assert!(!s.sem_try_take(sem));
    }

    #[test]
    fn equally_urgent_waiters_are_served_in_arrival_order() {
        let mut s = sched();
        let a = s.add_task("a", 3, 0).unwrap();
        let b = s.add_task("b", 3, 0).unwrap();
        let sem = s.new_semaphore(0, 4).unwrap();
        s.current = b;
        s.block_current_on(sem, FOREVER);
        s.current = a;
        s.block_current_on(sem, FOREVER);
        assert_eq!(s.sem_give(sem), Some(b));
        assert_eq!(s.sem_give(sem), Some(a));
    }

    #[test]
    fn a_wait_times_out_and_says_so() {
        let mut s = sched();
        let t = s.add_task("t", 2, 0).unwrap();
        let sem = s.new_semaphore(0, 1).unwrap();
        s.current = t;
        s.block_current_on(sem, 5);
        for _ in 0..4 {
            s.tick();
            assert_eq!(s.tasks[t].state, State::Blocked);
        }
        s.tick();
        assert_eq!(s.tasks[t].state, State::Ready);
        assert!(s.tasks[t].timed_out);
        // A later give finds nobody waiting.
        assert_eq!(s.sem_give(sem), None);
        // A wait that is satisfied is not a timeout.
        s.block_current_on(sem, 5);
        assert_eq!(s.sem_give(sem), Some(t));
        assert!(!s.tasks[t].timed_out);
    }

    #[test]
    fn priority_inheritance_lets_the_owner_finish_ahead_of_a_medium_task() {
        // Classic inversion: L owns the mutex, H wants it, M would starve L.
        let mut s = sched();
        let l = s.add_task("L", 1, 0).unwrap();
        let m = s.add_task("M", 3, 0).unwrap();
        let h = s.add_task("H", 5, 0).unwrap();
        let mx = s.new_mutex().unwrap();
        s.current = l;
        assert!(s.mutex_try_take(mx));
        s.tasks[h].state = State::Free; // H is not running yet
        s.tasks[h].state = State::Ready;
        s.current = h;
        assert!(!s.mutex_try_take(mx));
        s.mutex_boost_owner(mx);
        s.block_current_on(mx, FOREVER);
        assert_eq!(s.tasks[l].prio, 5, "L inherits H's priority");
        // With H blocked, M (3) is ready too, but L (now 5) runs first.
        s.current = m;
        assert_eq!(s.pick_next(), l);
        // L releases: ownership passes to H, L drops back to 1.
        assert_eq!(s.mutex_give(mx, l), Ok(Some(h)));
        assert_eq!(s.objs[mx].owner, Some(h));
        assert_eq!((s.tasks[l].prio, s.tasks[h].prio), (1, 5));
        // Only the owner can release.
        assert_eq!(s.mutex_give(mx, l), Err(NotOwner));
    }

    #[test]
    fn inheritance_follows_a_chain_of_owners() {
        let mut s = sched();
        let a = s.add_task("a", 1, 0).unwrap(); // owns m1
        let b = s.add_task("b", 2, 0).unwrap(); // owns m2, waits for m1
        let c = s.add_task("c", 6, 0).unwrap(); // waits for m2
        let (m1, m2) = (s.new_mutex().unwrap(), s.new_mutex().unwrap());
        s.current = a;
        assert!(s.mutex_try_take(m1));
        s.current = b;
        assert!(s.mutex_try_take(m2));
        s.mutex_boost_owner(m1);
        s.block_current_on(m1, FOREVER);
        s.current = c;
        s.mutex_boost_owner(m2);
        s.block_current_on(m2, FOREVER);
        assert_eq!((s.tasks[b].prio, s.tasks[a].prio), (6, 6));
    }
}
