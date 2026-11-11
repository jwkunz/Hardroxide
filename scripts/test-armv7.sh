#!/usr/bin/env bash
# Run the runtime crates' tests as armv7 (Zynq-7000, 32-bit) binaries under
# qemu user-mode emulation. Catches 32-bit issues (usize/off_t widths,
# alignment) without a board.
#
# Needs an arm-linux-gnueabihf cross gcc and qemu-arm. Defaults, in order:
#   ARM_GCC / QEMU_ARM / ARM_SYSROOT from the environment;
#   the Debian/Ubuntu packages gcc-arm-linux-gnueabihf + qemu-user;
#   the toolchain and qemu bundled with Vitis under $XILINX_ROOT (default ~/AMD/2025.2).
set -euo pipefail
XILINX_ROOT=${XILINX_ROOT:-$HOME/AMD/2025.2}
vitis_gcc=$XILINX_ROOT/gnu/aarch32/lin/gcc-arm-linux-gnueabi/bin/arm-linux-gnueabihf-gcc
vitis_qemu=$XILINX_ROOT/data/emulation/qemu/comp/qemu/sysroots/x86_64-petalinux-linux/usr/bin/qemu-arm

ARM_GCC=${ARM_GCC:-$(command -v arm-linux-gnueabihf-gcc || echo "$vitis_gcc")}
QEMU_ARM=${QEMU_ARM:-$(command -v qemu-arm || command -v qemu-arm-static || echo "$vitis_qemu")}
[ -x "$ARM_GCC" ] || { echo "no arm-linux-gnueabihf-gcc (set ARM_GCC)" >&2; exit 1; }
[ -x "$QEMU_ARM" ] || { echo "no qemu-arm (set QEMU_ARM)" >&2; exit 1; }
if [ -z "${ARM_SYSROOT:-}" ]; then
  ARM_SYSROOT=$("$ARM_GCC" -print-sysroot)
  # Debian's cross gcc reports an empty sysroot; its libraries live here.
  [ -n "$ARM_SYSROOT" ] && [ "$ARM_SYSROOT" != / ] || ARM_SYSROOT=/usr/arm-linux-gnueabihf
fi

rustup target add armv7-unknown-linux-gnueabihf >/dev/null
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=$ARM_GCC
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUNNER="$QEMU_ARM -L $ARM_SYSROOT"
# hardroxide-gen and -cli run on the host, not the board, and their tests
# spawn cargo, so they are left out.
exec cargo test --target armv7-unknown-linux-gnueabihf \
  -p hardroxide-hwh -p hardroxide-mmio -p hardroxide-fpga -p hardroxide-dma \
  -p hardroxide-irq -p hardroxide-ip "$@"
