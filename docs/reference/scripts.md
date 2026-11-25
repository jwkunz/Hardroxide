# Board scripts

The scripts in `scripts/board/` are for the **Linux** flavor. Bare-metal and RTOS programs are run with
`hardroxide run` and `hardroxide package` ([CLI](cli.md)) instead, and tested with `scripts/test-qemu.sh` (QEMU),
which sits next to `test-armv7.sh`, `check-no-std.sh` (the runtime crates build for `armv7a-none-eabihf` and `aarch64-unknown-none`) and
`build-site.sh` (the documentation site into `target/site`), and `test-board.sh`: the board regression, which generates, builds and runs
the self-checking bare-metal and RTOS programs of the tutorials on a board over JTAG and checks each one's console output
(`scripts/test-board.sh [filter]`; needs the Vitis tools and the board in JTAG boot mode). It runs the video, audio and RTOS audio programs always (a monitor on HDMI OUT and headphones are only needed to see and hear them; the checks read the console). Three
environment variables switch on what needs something connected: `HRX_AUDIO_LOOPBACK=1` (a cable from HPH OUT to LINE IN, for the audio loopback tests), `HRX_NET=1` (an Ethernet cable to a PC or router, for `net_ping`, `net_web`, `net_https` and the integrated `rt_station`) and
`HRX_SD_SERIAL=0x...` (a card **that may be erased**, named by the serial number `sd_info` prints: the SD tests format it, and the format program refuses any other card that holds data). `scripts/test-qemu.sh` also runs the SD card (with `fsck.fat` on the image it wrote) and the network stack
(DHCP, HTTP, HTTPS, UDP and TCP echo, against real `curl`, `openssl s_client` and Python sockets) under QEMU, and needs `dosfstools`, `curl`, `openssl` and Python 3.

## Zynq UltraScale+

`scripts/test-qemu-psu.sh` runs `baremetal/psu-smoke` on QEMU's `xlnx-zcu102` machine (`test-qemu.sh` ends by calling it).
`scripts/test-board-rfsoc.sh` runs it on an RFSoC 4x2 in JTAG boot mode, with the loopback fixture in the PL and again without
programming it, and then the kernel's test, `baremetal/rt-smoke` (which `test-qemu-psu.sh` also runs). The designs themselves are checked without Hardroxide code by `fixtures/rfsoc4x2/scripts/probe.py` (with
`HRX_RF_KEEP_CLOCKS=1` it leaves a running board's clock chips alone, to check what a program set up). The board script
also runs `baremetal/rfsoc-board`, the demonstration of the board's own hardware.

## `check-compat.py`: backward compatibility

`scripts/check-compat.py [semver] [generator] [programs]` (all three by default; CI runs it) fails if anything a user of the published release (2.7.0; `COMPAT_BASE` to change it) relies on has changed, which
[ADR 0005](../decisions/0005-ultrascale-plus.md) promises will not happen:

- **semver:** every library's public API against the crate on crates.io, with `cargo-semver-checks`, and for `hardroxide-ps7` and `hardroxide-rt` also as built for `armv7a-none-eabihf`, where their Arm-only modules exist.
- **generator:** for every fixture design and all three platforms, the working tree's `generate` gives the same files as the published CLI (only the version in the dependencies may differ), `update` accepts the published CLI's crate and leaves it alone, and no command or flag has gone.
- **programs:** crates made by the published CLI, with the programs of that release (read from the tag), build against the working tree's libraries.

It needs `cargo install cargo-semver-checks --locked`, the `armv7a-none-eabihf` target and the release's git tag; the first run installs the published CLI under `target/compat/`.

## Board helpers

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
a board; CI uses it. See [Getting started](../guide/getting-started.md#build-for-the-board-linux-flavor).
