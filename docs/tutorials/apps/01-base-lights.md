# Applications 1: Lights and buttons on the base overlay

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** bare metal and RTOS (the real-time kernel)

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 30 minutes · **Needs:** [Bare metal 1](../baremetal/01-jtag.md)

The PYNQ base overlay is the design every PYNQ-Z2 user already has: HDMI in and out, an audio codec, four MicroBlaze
subsystems, and the board's buttons, switches and LEDs behind an interrupt controller. This tutorial brings it up with no
operating system, then under the kernel, using only the buttons, switches and LEDs. Two things are new: a hardware
interrupt that goes through an **AXI interrupt controller** to the processor, and the kernel's **`IrqEvent`**, which lets a
task sleep until a hardware interrupt fires.

## 1. Generate and check

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal -o work/base --hardroxide-path . --name design_1
generated 14 files in work/base
$ cd work/base && cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/board_check --xsa ../../fixtures/board/pynq_base_overlay.xsa
== board check (base) ==
PL configured, interface c52ebef9797f3034499e0948a5b9e93ca326eeb6367a4d69b5630111b5195f09
FCLK0: 100000000 Hz (want 100000000) ok
FCLK1: 142857142 Hz (want 142857132) ok
FCLK2: 200000000 Hz (want 200000000) ok
FCLK3: 100000000 Hz (want 100000000) ok
irq system_interrupts.irq -> GIC id 61 (level)
all checks passed; next run the per-IP examples
```

The overlay's own XSA has no `ps7_init`, so the repository ships a board-ready copy ([why](../../reference/base-overlay.md)). The
last line shows how the interrupt reaches the processor: every interrupt in the overlay goes through one controller,
`system_interrupts`, whose single output is GIC interrupt 61.

## 2. A self-test with no button presses

Copy `apps/base/baremetal/selftest.rs` into `examples/`. It checks the four clocks, walks the LEDs and RGB LEDs, and tests the interrupt
path with **no device involved**: the interrupt controller's *simulation mode* lets software raise an input, which must reach the
processor through the GIC exactly like a hardware interrupt:

```rust,ignore
intc.start_simulation().unwrap();       // hardware inputs ignored; software can raise inputs
intc.enable(SOFT).unwrap();             // an input nothing in the design drives
irq.enable().unwrap();
intc.simulate(SOFT).unwrap();           // raise it
let got = irq.wait_event(Some(Duration::from_millis(100)));   // controller -> GIC -> processor
```

```console
$ cargo build --release --example selftest
$ hardroxide run target/armv7a-none-eabihf/release/examples/selftest --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 8
running; console below (8 s) ...
== base overlay self-test ==
PASS init (PL configured, MMU and caches on)
PASS fclk within 1 % of the design
     FCLK0: 100000000 Hz (design 100000000)
PASS fclk within 1 % of the design
     FCLK1: 142857142 Hz (design 142857132)
PASS fclk within 1 % of the design
     FCLK2: 200000000 Hz (design 200000000)
PASS fclk within 1 % of the design
     FCLK3: 100000000 Hz (design 100000000)
PASS leds walk and read back
PASS rgb leds walk
PASS intc enable mask reads back
PASS interrupt to GIC controller -> GIC -> processor
     latency (write to wake-up, polled): 4 us
PASS intc pending input pending
PASS intc ack cleared
PASS buttons idle no press, nothing pending (do not touch the buttons)
SELFTEST OK
```

This is the program `scripts/test-board.sh` runs: it needs no one at the board.

## 3. Buttons through the interrupt controller (bare metal)

Copy `apps/base/baremetal/lights.rs`. A press raises the buttons' GPIO interrupt, which is input 13 of the controller. The loop
sleeps in `intc.wait` until the controller's output arrives, then follows the protocol for a level-sensitive interrupt: **clear the
source first, then acknowledge the controller**, or the input latches again at once:

```rust,ignore
btns.enable_interrupt(Channel::One).unwrap();   // the GPIO raises its interrupt on a pin change
intc.enable(BTNS_BIT).unwrap();
...
intc.wait(&irq, BTNS_BIT, Some(Duration::from_millis(500)))?;
sleep(Duration::from_millis(20));               // contacts bounce for a few milliseconds
let state = btns.read(Channel::One)? & 0xF;
btns.clear_interrupt(Channel::One)?;            // clear the source first...
intc.ack(BTNS_BIT)?;                            // ...then the controller
```

Run it and press the buttons (it stops after 60 s): button 0 counts up on the LEDs, 1 counts down, 2 clears, 3 lights both RGB LEDs in
the colour the switches choose (00 off, 01 red, 10 green, 11 blue). Output from a real run (a press of each button, switches at 00 and then 01, then button 0 four times):

```console
$ cargo build --release --example lights
$ hardroxide run target/armv7a-none-eabihf/release/examples/lights --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 65
running; console below (70 s) ...

== lights: press the buttons (60 s) ==
buttons 1000 pressed: LEDs 0000, switches 00
buttons 0100 pressed: LEDs 0000, switches 00
buttons 0010 pressed: LEDs 1111, switches 00
buttons 0001 pressed: LEDs 0000, switches 00
...
buttons 0001 pressed: LEDs 0001, switches 01
buttons 0001 pressed: LEDs 0010, switches 01
buttons 0001 pressed: LEDs 0011, switches 01
buttons 0001 pressed: LEDs 0100, switches 01
done
```

The first line of each pair is the pressed button's bit: `0010` is button 1, which counts down from 0 to `1111`.

The GPIO's interrupt registers are `AxiGpio::enable_interrupt`, `interrupt_pending` and `clear_interrupt` (the status register toggles on a
write of 1, which `clear_interrupt` accounts for).

## 4. A mood light under the kernel

Back at the repository's root, generate an RTOS crate and copy in `rt_lights.rs` and the program of the next section
(the other programs of `apps/base/rtos/` need further libraries, which their tutorials add):

```console
$ cd ../..
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform rtos -o work/baser --hardroxide-path . --name design_1
$ cp apps/base/rtos/rt_lights.rs apps/base/rtos/rt_irq_latency.rs work/baser/examples/
$ cd work/baser && cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/rt_lights --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 16
running; console below (16 s) ...

== mood light: button 0 pattern, 1/2 brightness, 3 off/on ==
mood light: mode 0, brightness 8/8, 0 button presses, pwm task worst lateness 1 us
mood light: mode 0, brightness 8/8, 0 button presses, pwm task worst lateness 1 us
mood light: mode 0, brightness 8/8, 0 button presses, pwm task worst lateness 1 us
```

With buttons pressed during a minute (the status line every five seconds):

```console
mood light: mode 1, brightness 7/8, 5 button presses, pwm task worst lateness 7 us
mood light: mode 2, brightness 6/8, 8 button presses, pwm task worst lateness 7 us
mood light: mode 0, brightness 4/8, 16 button presses, pwm task worst lateness 7 us
```

Four tasks, each with a deadline of its own:

| Task | Priority | Job |
|---|---|---|
| `buttons` | 6 | sleeps on an `IrqEvent`; wakes within microseconds of a press, debounces, updates the pattern and brightness |
| `pwm` | 5 | every 1 ms writes the RGB LEDs: 8 steps give 125 Hz pulse-width modulation, so an on/off GPIO shows eight brightness levels |
| `colour` | 3 | every 20 ms moves the colour (a rainbow, a breathing white, red or green) |
| `report` | 2 | prints a status line every 5 seconds |

The RGB LEDs fade through a rainbow. Press button 0 for the next pattern, 1 and 2 for brightness, 3 to switch off and on. The
`pwm` task reports its worst lateness against the 1 ms grid: 1 µs idle, 7 µs while buttons are being pressed.

The key lines are the `IrqEvent`: the task sleeps, the kernel wakes it, and there is no handler to write:

```rust,ignore
let ev = IrqEvent::for_line(&design_1::pac::system_interrupts::IRQ_IRQ).unwrap();
ev.rearm();                      // unmask the interrupt in the GIC
loop {
    ev.wait(WAIT_FOREVER);       // sleep until a press
    // ...debounce, read the buttons, clear the GPIO flag, acknowledge the controller...
    ev.rearm();                  // only now unmask: the source is quiet
}
```

## 5. How long does an interrupt take to reach a task?

`apps/base/rtos/rt_irq_latency.rs` raises the controller's simulated interrupt 2000 times while a priority-1 task never sleeps and a
priority-2 task walks a megabyte of memory, and times each one from the write to the priority-6 task running, with the 3 ns global timer:

```console
$ hardroxide run target/armv7a-none-eabihf/release/examples/rt_irq_latency --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 20
== interrupt-to-task latency under load ==
2000 interrupts, interrupt controller to task:
  mean 4247 ns, worst 6225 ns
     0 to    5 us: 1999
     5 to    8 us: 1
     8 to   12 us: 0
    ...
LATENCY OK
```

A hardware interrupt reaches a task in about 4 µs and never more than 6.2 µs here, whatever else is running. That is what the
kernel is for.

## 6. Run everything at once

```console
$ scripts/test-board.sh
PASS  baremetal/gpio board_check
PASS  rtos/gpio control loop
...
board regression: 10 passed, 0 failed
```

The script generates, builds and runs the self-checking programs of every tutorial so far on the board and checks each one's
console output.

Next in the series (see [ADR 0004](../../decisions/0004-base-overlay-applications.md)): the HDMI output and a graphics library.
