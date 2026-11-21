# Bare metal 1: Run over JTAG

**Design:** `fixtures/board/pynq_z2_gpio.xsa` · **Time:** 20 minutes · **Needs:** the
[bare-metal host setup](index.md#host-setup) (Vitis 2025.2, the JTAG udev rule, the board in
JTAG boot mode)

The [Linux tutorials](../linux/index.md) run under Linux. Here the same GPIO design runs with **no operating system**: your
program is the only software on the processor, started over JTAG by `hardroxide run`. The design is
the board-accurate variant in `fixtures/board/` (the plain fixtures leave the PS UART disabled, so a
bare-metal program built from them could not print).

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
$ hardroxide generate fixtures/board/pynq_z2_gpio.xsa --platform baremetal -o work/bm --hardroxide-path .
generated 10 files in work/bm
$ cd work/bm && ls
build.rs  Cargo.toml  examples  interface.sha256  interface.txt  link.ld  src  (and .cargo/config.toml)
```

There is no bitstream in the crate: `hardroxide run` takes it from the XSA. `link.ld` places the program in DDR and
gives each processor mode a stack; `.cargo/config.toml` selects the `armv7a-none-eabihf` target
(`rustup target add armv7a-none-eabihf` once).

## 3. The first check

```console
$ cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/board_check --xsa ../../fixtures/board/pynq_z2_gpio.xsa
running; console below (10 s) ...

== board check (design_1) ==
PL configured, interface 0ec1a9e337a346ed08114c3c125241809c6a9100b5f6f79127ddc5fcbab56471
FCLK0: 100000000 Hz (want 100000000) ok
all checks passed; next run the per-IP examples
```

`hardroxide run` reset the board, ran the XSA's own `ps7_init`, programmed the PL, loaded the program and started
it. `Design::init()` turned on the MMU and caches, brought up the console and mapped the GPIO, and failed with a clear error
rather than hanging if the PL were not configured.

## 4. Your own program

Copy `fixtures/board_tests_bm/baremetal_gpio/leds.rs` into `examples/`. It mirrors the buttons onto the LEDs
for 20 seconds and prints each change:

```rust,ignore
let gpio = &d.axi_gpio_0;
gpio.set_input_mask(Channel::One, 0).unwrap(); // channel 1: LEDs, all outputs
let mut last = u32::MAX;
for _ in 0..400 {
    let inputs = gpio.read(Channel::Two).unwrap(); // buttons 3:0, switches 5:4
    gpio.write(Channel::One, inputs & 0xF).unwrap();
    if inputs != last {
        println!("buttons {:04b}, switches {:02b}", inputs & 0xF, inputs >> 4);
        last = inputs;
    }
    sleep(Duration::from_millis(50));
}
```

```console
$ cargo build --release --examples
$ hardroxide run target/armv7a-none-eabihf/release/examples/leds --xsa ../../fixtures/board/pynq_z2_gpio.xsa --watch 25
running; console below (25 s) ...

== leds: buttons and switches -> LEDs for 20 s (press something) ==
buttons 0000, switches 00
```

Press buttons while it runs and each change is printed, with the LEDs following. A program is `ps7::entry!(main);
fn main() -> ! { ... }`: it never returns, and `halt()` parks the core. The driver calls are exactly those of
[Linux tutorial 1](../linux/01-gpio.md); only the setup (`Design::init()` instead of `Design::load()`) and `no_std` differ.

## 5. Interrupts and DMA

The other designs work the same way. Generate a crate from `fixtures/board/pynq_z2_peripherals.xsa` for
`timer_irq` (a PL interrupt reaching a bare-metal program through the GIC) or from
`fixtures/board/pynq_z2_dma_loopback.xsa` for `dma_loopback` (buffers from `static_dma_buffer!`, which the MMU maps
non-cacheable, so no cache maintenance is needed):

```console
$ hardroxide run .../examples/dma_loopback --xsa fixtures/board/pynq_z2_dma_loopback.xsa
DMA loopback OK: 4096 bytes
```

## 6. Booting without the cable

```console
$ hardroxide package target/armv7a-none-eabihf/release/examples/leds --xsa fixtures/board/pynq_z2_gpio.xsa -o BOOT.BIN
```

builds a `BOOT.BIN` (see [Booting on its own](../../guide/baremetal.md#booting-on-its-own)).
