# Bare-metal tutorials

`--platform baremetal`: a `no_std` program that is the only software on core 0 of the Zynq. Startup code,
exception vectors, the interrupt controller, timers, the UART console, the MMU and the caches come from
[`hardroxide-ps7`](../../reference/ps7.md); the register layer and the typed IP drivers are the same ones the
Linux flavor uses.

## Host setup

You need the AMD **Vitis** tools (2025.2 was used; `hardroxide run` drives the board through `xsdb`), a JTAG
udev rule, and the PYNQ-Z2 in **JTAG boot mode**. The full steps, including the udev rule and
`hardroxide doctor`, are in [Running bare metal](../../guide/baremetal.md). If the Vitis tools are missing the
commands say so and stop.

Running a program takes over the board: Linux and `ssh` are gone until the next SD-card boot, and `run` always
recovers it with a system reset, so there is no board prep and no power cycling.

## Extras

- The tutorials use `fixtures/board/pynq_z2_gpio.xsa`, the board-accurate variant whose PS UART is enabled. A design
  without a board preset prints nothing on the console ([why](../../guide/baremetal.md#workflow)).
- No cross linker is needed: bare-metal crates build with `cargo` for `armv7a-none-eabihf`
  (`rustup target add armv7a-none-eabihf`).
- To start a program without a host, see [Booting on its own](../../guide/baremetal.md#booting-on-its-own)
  (`hardroxide package`).

## Tutorials

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Run over JTAG](01-jtag.md) | generate, build, `hardroxide run`, the console |

The generated crate also contains examples for timers, PL interrupts and DMA (`timer_delay`, `timer_irq`,
`dma_loopback`); they are described in the [bare-metal crate reference](../../reference/baremetal.md).
Next: the [RTOS tutorials](../rtos/index.md).
