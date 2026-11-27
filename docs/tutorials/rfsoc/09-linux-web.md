# RFSoC 9: A web page for the board

**Board:** [RFSoC 4x2](../../boards/rfsoc-4x2.md) · **Runs:** Linux

What Linux adds over bare metal: the network and everything else in Rust's standard library, and drivers the kernel
already has. `apps/rfsoc4x2/linux/dashboard.rs` serves a page showing the FPGA's temperature, the buttons, the switches
and the nine power monitors, with controls for the LEDs.

## Run it

With the crate of [tutorial 8](08-linux.md), forward a port of your computer to the board while the program runs:

```console
$ export BOARD=xilinx@192.168.2.99
$ HRX_SSH='ssh -L 8080:127.0.0.1:8080' scripts/board/run.sh ../base-linux dashboard
```

```text
the board's page is at http://127.0.0.1:8080/ (Ctrl-C to stop)
```

Open <http://127.0.0.1:8080/> on your computer. The numbered buttons toggle the white LEDs, the named ones colour the two
colour LEDs, and the page reads the board again every second, so a switch you move shows up by itself.

By default the program only listens inside the board, and the SSH connection is what brings the page to you. Giving it an
address, `dashboard 192.168.2.99:8080`, makes it reachable from the network **with no password**: anyone who can reach
it can set the LEDs, and the program runs as root. Do that only on a network of your own.

## Where the numbers come from

`http://127.0.0.1:8080/state` is what the page reads (shortened):

```text
{"celsius":50.28,"buttons":0,"switches":0,"leds":0,"colour":0,"power":[
 {"name":"VCC0V85","volts":0.888,"amps":6.77,"watts":5.8}, ...]}
```

| Shown | Read from | By |
|---|---|---|
| buttons, switches, LEDs | AXI GPIO blocks in the FPGA | the generated crate: `d.sws_gpio.read(Channel::One)` |
| temperature | the FPGA's system monitor block | a register of the generated crate's window |
| power | INA220 monitors on the processor's I2C bus | Linux's own driver, as files in `/sys/class/hwmon` |

The power monitors are not part of the FPGA design, and Linux already drives them, so the program reads files; on bare
metal [tutorial 3](03-board-hardware.md) drives the same chips with Hardroxide's library. The nine rails summed to
12.5 W with this design in both.

## How it is put together

- One request at a time, on one thread: the design needs no lock. A client that says nothing is dropped after two
  seconds.
- The listener is opened **before** the bitstream is loaded, so a port already in use fails without touching the FPGA.
- The program understands three requests and a number each (`/state`, `/leds?value=0..15`, `/colour?value=0..7`);
  everything else is refused.

Try this: add a button that runs the LEDs as a counter, then decide where the counting should happen so that the page
stays responsive.

Next: [the FPGA's own memory](10-linux-memory.md).
