# RTOS tutorials

`--platform rtos`: the bare-metal crate plus the [`hardroxide-rt`](../../reference/rt.md) kernel, a small
FreeRTOS-style kernel written in this repository with no third-party dependencies. Tasks have fixed priorities
and a more urgent one preempts a less urgent one at once; there are semaphores, priority-inheritance mutexes,
queues and drift-free periodic releases.

## Setup

Exactly the [bare-metal setup](../baremetal/index.md#host-setup): Vitis tools, the JTAG udev rule, the board in
JTAG boot mode, and `fixtures/board/pynq_z2_gpio.xsa`. Do [Bare metal 1](../baremetal/01-jtag.md) first; it
shows the build-and-run loop these tutorials reuse.

## Tutorials

| # | Tutorial | You learn |
|---|---|---|
| 1 | [A 10 ms control loop on the kernel](01-control-loop.md) | tasks, priorities, `Periodic`, `Shared` |

How well the kernel keeps its deadlines under load is measured in
[Verification gaps](../../08-verification-gaps.md): over two hours with a processor hog and a cache-thrashing task
running, the 10 ms task was released at most 10.2 us late (mean 1.06 us) with no missed release, and an interrupt reached
its waiting task within 5.4 us.
