# RFSoC 6: The kernel's tests and timing

[`hardroxide-rt`](../../reference/rt.md) is the same kernel on both processor families, with the same API.
`baremetal/rt-smoke` is one program for both.

```console
$ cd baremetal/rt-smoke
$ cargo build --release --target aarch64-unknown-none
$ hardroxide run target/aarch64-unknown-none/release/rt-smoke \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 15
```

```text
== hardroxide-rt smoke test ==
[0] before start: give and take true, a take that would wait false
[0] before start: an interrupt was handled, main went on and no task ran
[1] periodic: 20 releases on a 10 ms grid, worst lateness 0 ticks (no drift)
[3] queue: 10/10 items received in order (capacity 4, producer faster than the consumer's start)
[4] interrupt: semaphore given from an ISR woke the task: true, after 20 ticks
[2] preemption: 20 wakeups of a 5 ms sleeper over a spinning task, 0 late; spinner made some progress
[5] priority inheritance: the high task got the lock 30 ticks after the low task took it (inheritance worked)
[6] irq event: first true after 40 ticks, second (after rearm) true, nothing pending true
[7] registers: 200 checks in two tasks switching under them, 0 broken (preserved)
RT ALL OK
```

(It ends with a deliberate exception, as in tutorial 1.)

## Timing under load

`baremetal/rt-jitter` runs a 10 ms periodic task and an interrupt-to-task hand-off while other tasks thrash memory and
print, and reports histograms every minute. A two-hour run on this board:

| Measured | Mean | Worst | Samples |
|---|---|---|---|
| the 10 ms task released after its due time | 0.77 us | 5.94 us | 720,223 |
| from raising an interrupt to the waiting task running | 3.04 us | 4.65 us | 900,407 |

No release was missed, and neither worst case grew after minute 45. The run did not include interrupts from the PL, a
second master using DDR, or temperature extremes.

## What differs from the Zynq-7000

- Task stacks need at least 2048 bytes: a saved context, with all the vector registers, is 800 bytes.
- The tick comes from the 33.333 MHz system counter; periods differ by one count so that the average is exact.

There is no network program on the kernel for this board yet.

Next: [network and SD card](07-network-sd.md).
