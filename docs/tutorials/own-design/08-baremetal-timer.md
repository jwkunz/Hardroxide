# Your own design 8: A timer's interrupt on bare metal

**Design:** `fixtures/board/pynq_z2_peripherals.xsa` · **Time:** 15 minutes · **Needs:** [Bare metal 1](../baremetal/01-jtag.md)

Under [Linux](01-timer-spi-irq.md) an interrupt from the programmable logic arrives as a read on a UIO
device. With no operating system there is no UIO: the interrupt goes from the PL to the processor's interrupt controller (the
GIC), and `hardroxide-ps7` programs the GIC for you. The generated crate knows from the design which GIC line each
interrupt uses and whether it is level or edge triggered.

## 1. Generate and look at the routing

```console
$ hardroxide generate fixtures/board/pynq_z2_peripherals.xsa --platform baremetal -o work/bmp --hardroxide-path .
generated 11 files in work/bmp
$ cd work/bmp && ls examples
board_check.rs  timer_delay.rs  timer_irq.rs
```

`board_check` lists the interrupt routing (GIC lines 61 to 63 for this design, with their trigger types). The two timer
examples are generated because the design has an AXI Timer.

## 2. A hardware delay

```console
$ cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/timer_delay --xsa ../../fixtures/board/pynq_z2_peripherals.xsa --watch 8
running; console below (8 s) ...
100000000 Hz timer: asked 100 ms, took 100003 us
```

The program counts 100 ms in the PL timer and measures it against the processor's own time source: they agree to 3 us.

## 3. Wait for the interrupt

```console
$ hardroxide run target/armv7a-none-eabihf/release/examples/timer_irq --xsa ../../fixtures/board/pynq_z2_peripherals.xsa --watch 8
running; console below (8 s) ...
round 0: event Some(1) after 50004 us
round 1: event Some(2) after 50002 us
round 2: event Some(3) after 50003 us
```

The core of it:

```rust,ignore
let irq = IrqWait::for_line(&design_1::pac::axi_timer_0::IRQ_INTERRUPT).expect("interrupt reaches the PS");
d.enable_interrupts();                 // the GIC and the CPU's IRQ mask stay closed until now
t.start(ticks, false, true)?;          // one-shot, interrupt enabled
let ev = irq.wait_event(Some(Duration::from_secs(2)))?;
```

`IrqWait` is the bare-metal implementation of the `IrqWaiter` trait that every driver accepts (the Linux one reads a UIO
file). `wait_event` sleeps the core until the handler has seen the interrupt, then returns how many have arrived. Your code does
not write a handler.

## 4. Your own program: how regular is a periodic interrupt?

Copy `fixtures/board_tests_bm/baremetal_timer/timer_ticks.rs` into `examples/`. It starts the timer in auto-reload mode
at 10 ms and records the time between 200 consecutive interrupts:

```rust,ignore
timer.start(timer.ticks_for_ns(10_000_000) as u32, true, true).unwrap();   // periodic, interrupt on
for n in 1..=200 {
    irq.wait_event(Some(Duration::from_secs(1))).unwrap();
    let now = t0.elapsed().as_micros() as u64;
    timer.clear().unwrap();    // acknowledge the PL interrupt flag...
    irq.enable().unwrap();     // ...and unmask the line again
    // gap = now - last; keep the minimum and maximum
}
```

```console
$ cargo build --release --example timer_ticks
$ hardroxide run target/armv7a-none-eabihf/release/examples/timer_ticks --xsa ../../fixtures/board/pynq_z2_peripherals.xsa --watch 8
running; console below (8 s) ...

== timer_ticks: 200 interrupts at 10 ms ==
200 events in 2000007 us; gap between events: min 9999 us, max 10001 us
```

Two seconds for 200 events, and no gap more than 1 us from 10 ms: the period comes from the PL timer, so it does not drift
with whatever the processor is doing. Acknowledging the PL flag and re-enabling the line after each event is all this timer needs.

Next: [DMA](09-baremetal-dma.md). For several things happening at once, each with its own deadline, see the
[RTOS tutorials](../rtos/index.md).
