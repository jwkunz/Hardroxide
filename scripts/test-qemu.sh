#!/usr/bin/env bash
# Run the bare-metal smoke test (baremetal/ps7-smoke) on QEMU's Zynq-7000 machine and check
# what it prints: startup, console, clocks, time source, GIC and private-timer interrupts,
# and the exception report. QEMU does not model the PL; this tests the PS support only.
#
# QEMU: $QEMU_SYSTEM, or `qemu-system-aarch64` on PATH, or the one bundled with Vitis
# (which has the `xilinx-zynq-a9` machine; stock Debian/Ubuntu QEMU has `xilinx-zynq-a9` too).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/baremetal/ps7-smoke"
XILINX_ROOT=${XILINX_ROOT:-$HOME/AMD/2025.2}
VITIS_QEMU_DIR=$XILINX_ROOT/data/emulation/qemu/comp/qemu/sysroots/x86_64-petalinux-linux/usr
if [ -n "${QEMU_SYSTEM:-}" ]; then
  RUN=("$QEMU_SYSTEM")
elif command -v qemu-system-arm >/dev/null; then
  RUN=(qemu-system-arm)
elif [ -x "$VITIS_QEMU_DIR/bin/qemu-system-aarch64" ]; then
  # The bundled QEMU needs its own libraries, for this process only.
  RUN=(env "LD_LIBRARY_PATH=$VITIS_QEMU_DIR/lib:$VITIS_QEMU_DIR/../lib" "$VITIS_QEMU_DIR/bin/qemu-system-aarch64")
else
  echo "no QEMU with the xilinx-zynq-a9 machine (set QEMU_SYSTEM)" >&2; exit 1
fi
rustup target add armv7a-none-eabihf >/dev/null
cargo build --release --features qemu
ELF=target/armv7a-none-eabihf/release/ps7-smoke
OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none \
  -serial stdio -serial null -monitor none -kernel "$ELF" >"$OUT" 2>&1 || true
cat "$OUT"
fail=0
need() { grep -q -- "$1" "$OUT" || { echo "MISSING: $1" >&2; fail=1; }; }
need "== hardroxide-ps7 smoke test =="
need "core 0"
need "timer interrupts: 20 in"
need "unhandled interrupts: 0"
need "ALL OK"
need "EXCEPTION: undefined instruction"
# Timing: sleep(100 ms) within 5%, 20 ticks of 10 ms within 5% of 200 ms (virtual time).
python3 - "$OUT" <<'PY' || fail=1
import re, sys
t = open(sys.argv[1], errors="replace").read()
sleep_us = int(re.search(r"sleep\(100 ms\) took (\d+) us", t).group(1))
span_us = int(re.search(r"timer interrupts: 20 in (\d+) us", t).group(1))
ok = 95_000 <= sleep_us <= 105_000 and 190_000 <= span_us <= 210_000
print(f"timing: sleep {sleep_us} us, 20 ticks {span_us} us -> {'ok' if ok else 'OUT OF RANGE'}")
sys.exit(0 if ok else 1)
PY
[ "$fail" = 0 ] && echo "qemu smoke test: ok" || { echo "qemu smoke test: FAILED" >&2; exit 1; }

# ---- the kernel (hardroxide-rt) ----
cd "$ROOT/baremetal/rt-smoke"
cargo build --release --features qemu
OUT2=$(mktemp)
trap 'rm -f "$OUT" "$OUT2"' EXIT
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none \
  -serial stdio -serial null -monitor none -kernel target/armv7a-none-eabihf/release/rt-smoke >"$OUT2" 2>&1 || true
cat "$OUT2"
fail=0
need2() { grep -q -- "$1" "$OUT2" || { echo "MISSING: $1" >&2; fail=1; }; }
need2 "no drift"
need2 "10/10 items received in order"
need2 "woke the task: true, after 20 ticks"
need2 "0 late"
need2 "inheritance worked"
need2 "RT ALL OK"
[ "$fail" = 0 ] && echo "qemu kernel test: ok" || { echo "qemu kernel test: FAILED" >&2; exit 1; }
