# Running bare metal

A bare-metal program runs on core 0 of the Zynq with no operating system
([ADR 0003](../decisions/0003-bare-metal-and-rtos-platforms.md)). This page is the host setup and the
workflow; the crate itself is described in [Bare-metal crate](../reference/baremetal.md).

## Host setup (once)

You need the AMD **Vitis** tools (2025.2 was used): `hardroxide run` drives the board through
`xsdb` and `hw_server`. If they are missing the commands say so and stop. Point at them with
`HRX_VITIS` (the directory with `settings64.sh`) or `XILINX_ROOT`, or put `xsdb` on `PATH`.

**The JTAG cable's udev rule.** The board's USB port is an FT2232: JTAG on one interface, the UART
on the other. `hw_server` needs write access to it, which a normal user lacks. Install Vitis's rule
once, as root:

```console
$ sudo cp <Vitis install>/data/xicom/cable_drivers/lin64/install_script/install_drivers/52-xilinx-ftdi-usb.rules /etc/udev/rules.d/
$ sudo udevadm control --reload-rules && sudo udevadm trigger
```

then **unplug and replug the board's USB cable** (a rule only applies when a device is added).
Caveats: the rule makes **every** FTDI device (`0403`) world-writable, and as written it asks udev to
unbind `ftdi_sio` from both interfaces. On the validated host the unbind did not happen (both
`/dev/ttyUSB0` and `ttyUSB1` stayed and `hw_server` worked); if `/dev/ttyUSB1` disappears on yours,
rebind it as root (`echo -n <bus-id>:1.1 > /sys/bus/usb/drivers/ftdi_sio/bind`). Read the console
with **one** reader only: a second process on `/dev/ttyUSB1` splits the byte stream.

Check everything with:

```console
$ hardroxide doctor
[ ok ] Vitis tools: found at /home/you/AMD/2025.2/Vitis
[ ok ] JTAG cable (FT2232, 0403:6010): /dev/bus/usb/003/008 is accessible
[ ok ] console port /dev/ttyUSB1: present
[ ok ] hw_server: already running on port 3121
```

## The board

Put the PYNQ-Z2 into **JTAG boot mode** (the boot-mode jumper) and power it. Running a bare-metal
program takes over the board: Linux and `ssh` are gone until the next SD-card boot, and a reset
(`rst -system`, done by `run`) always recovers it without a power cycle.

## Workflow

```console
$ hardroxide generate design.xsa --platform baremetal -o my_design --hardroxide-path .
$ cd my_design && rustup target add armv7a-none-eabihf
$ cargo build --release --example board_check
$ hardroxide run target/armv7a-none-eabihf/release/examples/board_check --xsa ../design.xsa
== board check (design_1) ==
PL configured, interface 0ec1a9e3...
FCLK0: 100000000 Hz (want 100000000) ok
all checks passed; next run the per-IP examples
```

`run` resets the board, runs the design's own `ps7_init.tcl` from the XSA, programs the PL, loads the
ELF, starts it and shows the console. **The XSA's PS configuration matters**: the console needs the
PS UART's pins (MIO 14/15 on the PYNQ-Z2) and clocks, which `ps7_init` sets from the design. A design
built without a board preset has UART0 disabled and prints nothing; the fixture scripts in
`fixtures/scripts/board/` show how to add the PYNQ-Z2's configuration (everything except DDR: see
[Verification gaps](../08-verification-gaps.md)).

Always run `board_check` first: it touches no IP register, and `Design::init()` refuses with
`PlNotConfigured` instead of letting a register access on an unconfigured PL hang the CPU.
