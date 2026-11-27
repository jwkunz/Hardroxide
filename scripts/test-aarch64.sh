#!/usr/bin/env bash
# Run the runtime crates' tests as AArch64 Linux (Zynq UltraScale+) binaries under qemu user-mode emulation, as
# test-armv7.sh does for the Zynq-7000.
#
# Needs an aarch64-linux-gnu cross gcc and qemu-aarch64. Defaults, in order:
#   AARCH64_GCC / QEMU_AARCH64 / AARCH64_SYSROOT from the environment;
#   the Debian/Ubuntu packages gcc-aarch64-linux-gnu + qemu-user;
#   the toolchain and qemu bundled with the vendor tools under $XILINX_ROOT (default ~/AMD/2025.2).
set -euo pipefail
XILINX_ROOT=${XILINX_ROOT:-$HOME/AMD/2025.2}
vitis_gcc=$XILINX_ROOT/Vitis/gnu/aarch64/lin/aarch64-linux/bin/aarch64-linux-gnu-gcc
vitis_qemu=$XILINX_ROOT/data/emulation/qemu/comp/qemu/sysroots/x86_64-petalinux-linux/usr/bin/qemu-aarch64

AARCH64_GCC=${AARCH64_GCC:-$(command -v aarch64-linux-gnu-gcc || echo "$vitis_gcc")}
QEMU_AARCH64=${QEMU_AARCH64:-$(command -v qemu-aarch64 || command -v qemu-aarch64-static || echo "$vitis_qemu")}
[ -x "$AARCH64_GCC" ] || { echo "no aarch64-linux-gnu-gcc (set AARCH64_GCC)" >&2; exit 1; }
[ -x "$QEMU_AARCH64" ] || { echo "no qemu-aarch64 (set QEMU_AARCH64)" >&2; exit 1; }
if [ -z "${AARCH64_SYSROOT:-}" ]; then
  AARCH64_SYSROOT=$("$AARCH64_GCC" -print-sysroot)
  # Debian's cross gcc reports an empty sysroot; its libraries live here.
  [ -n "$AARCH64_SYSROOT" ] && [ "$AARCH64_SYSROOT" != / ] || AARCH64_SYSROOT=/usr/aarch64-linux-gnu
fi
# The vendor's sysroot keeps its libraries under usr/lib only; the emulator looks for them in lib too.
if [ ! -e "$AARCH64_SYSROOT/lib" ] && [ -e "$AARCH64_SYSROOT/usr/lib/ld-linux-aarch64.so.1" ]; then
  shim=$(cd "$(dirname "$0")/.." && pwd)/target/aarch64-sysroot
  mkdir -p "$shim"
  ln -sfn "$AARCH64_SYSROOT/usr/lib" "$shim/lib"
  ln -sfn "$AARCH64_SYSROOT/usr" "$shim/usr"
  AARCH64_SYSROOT=$shim
fi

rustup target add aarch64-unknown-linux-gnu >/dev/null
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=$AARCH64_GCC
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER="$QEMU_AARCH64 -L $AARCH64_SYSROOT"
# hardroxide-gen and -cli run on the host, not the board, and their tests spawn cargo, so they are left out.
exec cargo test --target aarch64-unknown-linux-gnu \
  -p hardroxide-hwh -p hardroxide-mmio -p hardroxide-fpga -p hardroxide-dma \
  -p hardroxide-irq -p hardroxide-ip "$@"
