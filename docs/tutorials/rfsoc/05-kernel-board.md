# RFSoC 5: The board's hardware as tasks

**Board:** [RFSoC 4x2](../../boards/rfsoc-4x2.md) · **Runs:** RTOS (the real-time kernel)

`baremetal/rt-board` drives the chips of [tutorial 3](03-board-hardware.md) from the kernel. Some of them are slow (a
display redraw takes milliseconds, setting up the clock chips half a second); the kernel keeps that from delaying
anything urgent.

```console
$ cd baremetal/rt-board
$ cargo build --release
$ hardroxide run target/aarch64-unknown-none/release/rt-board \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 60
```

```text
== RFSoC 4x2 board hardware on the kernel ==
Ethernet address xx:xx:xx:xx:xx:xx
clocks: programmed, both synthesizers locked: true
heartbeat: 19 beats, worst 188 ns from its time
button: worst 1040 ns from its time
monitor: 11 updates (0 incomplete), longest 10158 us
display: 10 draws, longest 6398 us, follows: true
clocks: done in 551 ms
  task        priority  processor  stack used
  (idle)             0       91.4%    560 of 4096
  heartbeat          6        0.0%    944 of 8192
  button             5        0.0%   1016 of 8192
  monitor            4        2.2%   1880 of 8192
  check              3        0.6%   1808 of 16384
  display            2        5.9%   1991 of 16384
RT BOARD OK
press PS_BTN to step through the display's pages
```

The first LED blinks twice a second; each press of `PS_BTN` shows the next page of the display.

## Read the numbers

The heartbeat and the button task are the most urgent, and ran within a microsecond or so of their times, although
`monitor` takes 10 ms for each round of readings, `display` 6 ms for each redraw, and `clocks` half a second. Those are
interrupted whenever something more urgent is due. The table (finished tasks are no longer in it) shows that the
processor is idle nine tenths of the time.

## Sharing without waiting

- Each chip, and so each bus, belongs to one task.
- The latest readings are shared in a `Shared<Readings>`, a value behind a mutex. `monitor` talks to the chips first and
  locks only to hand over the result; `display` locks only to copy it. A task that held the lock while talking to a slow
  chip would make everyone who wants the readings wait for that chip.
- The console is shared too: the `say!` macro locks it so that lines come out whole.
- Pins' directions are all set before the kernel starts. Afterwards tasks only write levels, and a level write changes
  its own pin alone.

The drivers' short waits are busy loops. In a low-priority task that only takes time nobody else wants; keep it in mind
if you move such a driver into an urgent task.

Next: [the kernel's tests and timing](06-kernel-timing.md).
