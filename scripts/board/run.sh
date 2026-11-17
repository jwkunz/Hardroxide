#!/bin/sh
# Run ON YOUR WORKSTATION: cross-build one example of a generated crate for the
# PYNQ-Z2, copy it to the board and run it as root.
#
#   scripts/board/run.sh <crate-dir> <example> [args...]
#
# Environment:
#   BOARD   ssh target                   (default xilinx@pynq)
#   LINKER  armv7 cross linker           (default: arm-linux-gnueabihf-gcc from PATH,
#                                         else the one bundled with Vitis)
#   HRX_SSH / HRX_SCP   replace ssh / scp (for example to add options)
# sudo on the stock PYNQ image asks for the password (xilinx) on the board.
set -eu
crate=${1:?usage: run.sh <crate-dir> <example> [args...]}
ex=${2:?usage: run.sh <crate-dir> <example> [args...]}
shift 2
BOARD=${BOARD:-xilinx@pynq}
SSH=${HRX_SSH:-ssh}
SCP=${HRX_SCP:-scp}
TARGET=armv7-unknown-linux-gnueabihf

if [ -z "${LINKER:-}" ]; then
  if command -v arm-linux-gnueabihf-gcc >/dev/null; then LINKER=arm-linux-gnueabihf-gcc
  else
    v=${XILINX_ROOT:-$HOME/AMD/2025.2}/gnu/aarch32/lin/gcc-arm-linux-gnueabi/bin/arm-linux-gnueabihf-gcc
    [ -x "$v" ] && LINKER=$v || { echo "no armv7 linker: apt install gcc-arm-linux-gnueabihf, or set LINKER" >&2; exit 1; }
  fi
fi
rustup target add $TARGET >/dev/null 2>&1 || true
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=$LINKER
( cd "$crate" && cargo build --release --target $TARGET --example "$ex" )
bin="$crate/target/$TARGET/release/examples/$ex"
$SCP "$bin" "$BOARD:" >/dev/null
# -t gives sudo a terminal for its password prompt.
$SSH -t "$BOARD" "sudo ./$ex $*"
