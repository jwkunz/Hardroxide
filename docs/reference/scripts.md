# Board scripts

The scripts in `scripts/board/` are for the **Linux** flavor. Bare-metal and RTOS programs are run with
`hardroxide run` and `hardroxide package` ([CLI](cli.md)) instead, and tested with `scripts/test-qemu.sh` (QEMU),
which sits next to `test-armv7.sh`, `check-no-std.sh` (the runtime crates build for `armv7a-none-eabihf`) and
`build-site.sh` (the documentation site into `target/site`), and `test-board.sh`: the board regression, which generates, builds and runs
the self-checking bare-metal and RTOS programs of the tutorials on a board over JTAG and checks each one's console output
(`scripts/test-board.sh [filter]`; needs the Vitis tools and the board in JTAG boot mode; set `HRX_AUDIO_LOOPBACK=1` with a cable from HPH OUT to LINE IN for the audio loopback tests).

Helper scripts in `scripts/board/` for working with a PYNQ-Z2. They are plain POSIX
shell, change only the running system, and are what the
[tutorials](../tutorials/index.md) use.

## `prep.sh`: free the PL (run on the board, as root)

```console
$ scp scripts/board/prep.sh $BOARD: && ssh -t $BOARD sudo sh prep.sh
stopped service jupyter
removed module panther_audio
unbound 41200000.gpio (gpio-xilinx)
ready: no driver is bound to a PL address
```

Reprogramming the PL while a kernel driver or service is using it can freeze the CPU
(see [G16](../08-verification-gaps.md)). `prep.sh` stops the systemd services in
`HRX_SERVICES` (default `panther-desktop jupyter pynq`), removes the kernel modules in
`HRX_MODULES` (default `panther_drm panther_audio zocl`) and unbinds every driver bound
to a platform device at a PL address (`0x40000000` to `0xBFFFFFFF`), except the passive
`uio_pdrv_genirq`. It exits non-zero and says which device if something stays bound.
Run it once per boot after the board has finished starting; a reboot restores
everything. `load()` independently refuses (`PlInUse`) if a driver is still bound.

## `overlay.sh`: apply the interrupt overlay (on the board, as root)

```console
$ scp work/design/interrupts.dtso $BOARD: && ssh -t $BOARD sudo sh overlay.sh interrupts.dtso
overlay status: applied
uio0: axi_timer_0_interrupt
```

Compiles the generated `interrupts.dtso` with `dtc` and applies it through configfs,
replacing the overlay this script applied before, then lists the UIO devices. A GIC
interrupt line keeps the trigger type (level or edge) of the first overlay that used
it until the board reboots; if a UIO device later returns `EIO`, reboot. See
[Interrupts](../guide/interrupts.md).

## `run.sh`: cross-build, copy and run (on your workstation)

```console
$ scripts/board/run.sh <crate-dir> <example> [args...]
```

Builds `<example>` of the generated crate in `<crate-dir>` for
`armv7-unknown-linux-gnueabihf`, copies the binary to the board's home directory and
runs it with `sudo`; extra arguments are passed to the program (for example an instance
name). Environment variables:

| Variable | Meaning | Default |
|---|---|---|
| `BOARD` | ssh target | `xilinx@pynq` |
| `LINKER` | cross linker | `arm-linux-gnueabihf-gcc` on `PATH`, else the one bundled with Vitis (`$XILINX_ROOT`, default `~/AMD/2025.2`) |
| `HRX_SSH`, `HRX_SCP` | replace `ssh` and `scp` (to add options or a wrapper) | `ssh`, `scp` |

`sudo` asks for the password on the board (`xilinx` on the stock image) each run.

## `test-armv7.sh`: runtime tests under emulation (on your workstation)

Runs the runtime crates' own tests as 32-bit ARM binaries under `qemu-arm`, without
a board; CI uses it. See [Getting started](../guide/getting-started.md#build-for-the-board).
