# RTOS 1: A 10 ms control loop on the kernel

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** RTOS (the real-time kernel)

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 20 minutes · **Needs:** [Bare metal 1](../baremetal/01-jtag.md) (the host setup)

Real-time control wants a loop that runs every 10 ms however busy the rest of the program is. `hardroxide-rt` is a
small kernel for that: tasks with priorities, where a more urgent task preempts a less urgent one at once.

## 1. Generate an RTOS crate

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform rtos -o work/base-rt --hardroxide-path . --name design_1
$ ls work/base-rt/examples
rt_control.rs
```

It is the bare-metal crate plus the kernel (`design_1::rt`). Only kernel programs are generated, because the kernel provides the
interrupt entries that the plain bare-metal programs use.

## 2. The program

Copy `apps/base/rtos/control.rs` into `examples/`. Two tasks share the design through a `Shared`, a value behind
a mutex with priority inheritance:

```rust,ignore
static DESIGN: Shared<Design<PhysMem>> = Shared::new();

fn control() {                         // priority 5
    let mut every = Periodic::every_ms(10);
    loop {
        every.wait_next();             // sleeps until the next 10 ms release: no drift
        let d = DESIGN.lock(WAIT_FOREVER).unwrap();
        let buttons = d.btns_gpio.read(Channel::One).unwrap() & 0xF;
        d.leds_gpio.write(Channel::One, buttons).unwrap();
    }
}

fn report() {                          // priority 2
    loop {
        delay_ms(1000);
        println!("control: {} runs, ...", RUNS.load(Relaxed));
    }
}

fn main() -> ! {
    let design = Design::init().unwrap();
    DESIGN.set(design);
    spawn("control", 5, task_stack!(8192).unwrap(), control).unwrap();
    spawn("report", 2, task_stack!(8192).unwrap(), report).unwrap();
    start(design_1::periph_hz().unwrap(), 1000)   // 1 kHz tick; never returns
}
```

Higher numbers are more urgent. `Periodic` releases on a fixed grid (`last + 10 ms` each time), so a slow iteration does not
shift later ones; `wait_next` returns `false` if an iteration overran its period.

## 3. Run it

```console
$ cd work/base-rt
$ cp ../../apps/base/rtos/control.rs examples/
$ cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/control --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 8

== control: buttons -> LEDs every 10 ms ==
control: 100 runs, worst deviation from the 10 ms period 1 us
control: 200 runs, worst deviation from the 10 ms period 1 us
control: 300 runs, worst deviation from the 10 ms period 1 us
control: 400 runs, worst deviation from the 10 ms period 1 us
control: 500 runs, worst deviation from the 10 ms period 1 us
control: 600 runs, worst deviation from the 10 ms period 1 us
```

The reporting task printing to the UART (slow) does not disturb the control loop, because the control task preempts it.
The measured deviation is from the global timer, so it shows how regular the releases are; interrupt-to-task
latency and long-run behaviour are listed in [Verification gaps](../../08-verification-gaps.md).

## 4. What else the kernel has

Semaphores (also given from interrupt handlers), bounded queues, delays and mutexes with priority inheritance: see
the [kernel reference](../../reference/rt.md). A PL interrupt reaches a task by a handler registered with
`ps7::irq::register` that calls `Semaphore::give_from_isr()`.
