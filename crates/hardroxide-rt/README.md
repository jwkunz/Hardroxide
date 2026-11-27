# hardroxide-rt

A FreeRTOS-style real-time kernel for the Zynq-7000's Cortex-A9 and the Cortex-A53 of Zynq UltraScale+, written for
[Hardroxide](https://github.com/jwkunz/Hardroxide): fixed-priority preemptive tasks with their
own stacks, delays and periodic release, semaphores, mutexes with priority inheritance and
bounded queues. `no_std`, static allocation, no third-party dependencies. The scheduling policy is
plain Rust, unit-tested on the host; only the context switch and the tick are specific to the processor.

Licensed under either of Apache-2.0 or MIT at your option.
