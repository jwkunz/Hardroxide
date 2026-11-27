# Bare metal 1: Run over JTAG

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** bare metal

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Time:** 20 minutes · **Needs:** the
[bare-metal host setup](index.md#host-setup) (Vitis 2025.2, the JTAG udev rule, the board in
JTAG boot mode)

The [Linux tutorials](../linux/index.md) run under Linux. Here the same base overlay runs with **no operating system**:
your program is the only software on the processor, started over JTAG by `hardroxide run`. The file in `fixtures/board/`
carries the board's start-up settings (`ps7_init`), which the program needs for its console
([why](../../reference/base-overlay.md)).

## 1. Check the host

```console
$ hardroxide doctor
[ ok ] Vitis tools: found at /home/you/AMD/2025.2/Vitis
[ ok ] JTAG cable (FT2232, 0403:6010): /dev/bus/usb/003/008 is accessible
[ ok ] console port /dev/ttyUSB1: present
[ ok ] hw_server: already running on port 3121
```

If a line says FAIL, the message says what to fix (usually the udev rule from the host setup page).

## 2. Generate a bare-metal crate

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal -o work/base-bm --hardroxide-path . --name design_1
$ cd work/base-bm && ls
```

There is no bitstream in the crate: `hardroxide run` takes it from the XSA. `link.ld` places the program in DDR and
gives each processor mode a stack; `.cargo/config.toml` selects the `armv7a-none-eabihf` target
(`rustup target add armv7a-none-eabihf` once).

## 3. The first check

```console
$ cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/board_check --xsa ../../fixtures/board/pynq_base_overlay.xsa
== board check (design_1) ==
PL configured, interface c52ebef9797f3034499e0948a5b9e93ca326eeb6367a4d69b5630111b5195f09
FCLK0: 100000000 Hz (want 100000000) ok
FCLK1: 142857142 Hz (want 142857132) ok
FCLK2: 200000000 Hz (want 200000000) ok
FCLK3: 100000000 Hz (want 100000000) ok
irq system_interrupts.irq -> GIC id 61 (level)
all checks passed; next run the per-IP examples
```

`hardroxide run` reset the board, ran the XSA's `ps7_init`, programmed the PL, loaded the program and started it.
`Design::init()` turned on the MMU and caches, brought up the console and mapped the blocks, and would have failed with a
clear error rather than hanging if the PL were not configured.

## 4. Your own program

```rust,ignore
{{#include ../../../apps/base/baremetal/leds.rs}}
```

Compare it with [Linux tutorial 1](../linux/01-lights.md): the driver calls are the same. What differs is the start
(`Design::init()` instead of `Design::load()`), no operating system (`#![no_std]`, `ps7::entry!(main)`, a `main` that
never returns) and `halt()` at the end, which parks the core.

```console
$ cp ../../apps/base/baremetal/leds.rs examples/
$ cargo build --release --example leds
$ hardroxide run target/armv7a-none-eabihf/release/examples/leds --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 25
```

Press the buttons and move the switches while it runs: the LEDs follow the buttons, the switches choose the colour of
the colour LEDs, and each change is printed. On a PYNQ-Z2, pressing each button and trying the switches:

```text
== leds: the LEDs, one at a time ==
== press the buttons and move the switches (20 s) ==
buttons 0000, switches 11
buttons 0001, switches 11
buttons 0000, switches 11
buttons 0010, switches 11
buttons 0000, switches 11
buttons 0100, switches 11
buttons 0000, switches 11
buttons 1000, switches 11
buttons 0000, switches 11
buttons 0000, switches 01
buttons 0000, switches 00
buttons 0000, switches 10
buttons 0000, switches 11
buttons 0000, switches 10
buttons 0000, switches 00
done
```

## 5. Booting without the cable

```console
$ cd ../..
$ hardroxide package work/base-bm/target/armv7a-none-eabihf/release/examples/leds --xsa fixtures/board/pynq_base_overlay.xsa -o BOOT.BIN
```

builds a `BOOT.BIN` (see [Booting on its own](../../guide/baremetal.md#booting-on-its-own)).

Next: [an interrupt](02-interrupt.md).
