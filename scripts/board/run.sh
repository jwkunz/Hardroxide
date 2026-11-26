#!/bin/sh
# Run ON YOUR WORKSTATION: cross-build one example of a generated crate for its
# board (Zynq-7000: armv7; Zynq UltraScale+: AArch64, told apart by the crate's
# bitstream/ports.cfg), copy it to the board and run it as root.
#
#   scripts/board/run.sh <crate-dir> <example> [args...]
#
# Environment:
#   BOARD   ssh target                   (default xilinx@pynq)
#   LINKER  cross linker for the board   (default: the GNU gcc for its processor from
#                                         PATH, else the one bundled with Vitis)
#   HRX_SSH / HRX_SCP   replace ssh / scp (for example to add options)
# sudo on the stock PYNQ image asks for the password (xilinx) on the board.
set -eu
crate=${1:?usage: run.sh <crate-dir> <example> [args...]}
ex=${2:?usage: run.sh <crate-dir> <example> [args...]}
shift 2
BOARD=${BOARD:-xilinx@pynq}
SSH=${HRX_SSH:-ssh}
SCP=${HRX_SCP:-scp}
if [ -f "$crate/bitstream/ports.cfg" ]; then
  TARGET=aarch64-unknown-linux-gnu
  gnu=aarch64-linux-gnu
  bundled=gnu/aarch64/lin/aarch64-linux/bin/aarch64-linux-gnu-gcc
else
  TARGET=armv7-unknown-linux-gnueabihf
  gnu=arm-linux-gnueabihf
  bundled=gnu/aarch32/lin/gcc-arm-linux-gnueabi/bin/arm-linux-gnueabihf-gcc
fi

if [ -z "${LINKER:-}" ]; then
  if command -v $gnu-gcc >/dev/null; then LINKER=$gnu-gcc
  else
    v=${XILINX_ROOT:-$HOME/AMD/2025.2}/$bundled
    [ -x "$v" ] || v=${XILINX_ROOT:-$HOME/AMD/2025.2}/Vitis/$bundled
    [ -x "$v" ] && LINKER=$v || { echo "no cross linker: apt install gcc-$gnu, or set LINKER" >&2; exit 1; }
  fi
fi
rustup target add $TARGET >/dev/null 2>&1 || true
export "CARGO_TARGET_$(echo $TARGET | tr 'a-z-' 'A-Z_')_LINKER=$LINKER"
( cd "$crate" && cargo build --release --target $TARGET --example "$ex" )
bin="$crate/target/$TARGET/release/examples/$ex"
$SCP "$bin" "$BOARD:" >/dev/null
# -t gives sudo a terminal for its password prompt.
$SSH -t "$BOARD" "sudo ./$ex $*"
