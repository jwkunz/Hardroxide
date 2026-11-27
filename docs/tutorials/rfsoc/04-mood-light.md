# RFSoC 4: A mood light on the kernel

**Board:** [RFSoC 4x2](../../boards/rfsoc-4x2.md) · **Runs:** RTOS (the real-time kernel)

`apps/rfsoc4x2/rtos/rt_lights.rs` is [tutorial 2](02-base-overlay.md)'s hardware run by the
[real-time kernel](../../reference/rt.md): several things happen at once, each by its own deadline. Generate the crate
again with the kernel in it:

```console
$ hardroxide generate fixtures/board/rfsoc4x2_base_overlay.xsa --platform rtos \
    --name design_1 -o ../base-rtos --hardroxide-path .
$ cp apps/rfsoc4x2/rtos/rt_lights.rs ../base-rtos/examples/
$ cd ../base-rtos
$ cargo build --release --example rt_lights
$ hardroxide run target/aarch64-unknown-none/release/examples/rt_lights \
    --xsa ../Hardroxide/fixtures/board/rfsoc4x2_base_overlay.xsa --watch 60
```

```text
== RFSoC 4x2 base overlay: a mood light on the kernel ==
buttons interrupt woke the buttons task: true, in 3 us
switches interrupt woke the buttons task: true, in 3 us
pwm task: 2400 runs, worst 5 us from its millisecond
RT LIGHTS OK

button 0 pattern, 1 brighter, 2 dimmer, 3 off and on
pattern 0, brightness 8/8, switches 0000, 0 presses, pwm worst 5 us
```

The two colour LEDs cycle through the rainbow. Button 0 changes the pattern (rainbow, breathing white, the colour that
switches 0 to 2 choose), buttons 1 and 2 change the brightness, button 3 switches the lights off and on, and the white
LEDs show the pattern's number.

## The tasks

A higher priority number is more urgent. The most urgent task that is ready runs, immediately.

| Task | Priority | Sleeps until | Then |
|---|---|---|---|
| `buttons` | 6 | the overlay's interrupt fires | reads the buttons and switches, changes the settings |
| `pwm` | 5 | the next millisecond | switches each colour channel on or off, which dims it |
| `selftest` | 4 | (runs once) | raises the interrupts from software and checks the way to `buttons` |
| `colour` | 3 | the next 20 ms | works out the colour |
| `report` | 2 | five seconds have passed | prints the state |

## Three ideas to take away

- **Sleep, do not poll.** `buttons` uses `IrqEvent`: it costs no processor time until the interrupt arrives, and runs a
  few microseconds after it. Clear the interrupt's source, then the controller, then call `rearm`.
- **`Periodic` does not drift.** `pwm` is released on a fixed grid, however long each run takes.
- **Share the design safely.** The tasks take the generated `Design` from a `Shared`, a lock with priority inheritance,
  for a few register accesses at a time, and never while they sleep.

Try this: give `colour` a higher priority than `pwm` and add a long loop to it, then watch what `report` says about the
`pwm` task.

Next: [the board's hardware as tasks](05-kernel-board.md).
