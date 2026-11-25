#!/usr/bin/env bash
# Run the Zynq UltraScale+ smoke test (baremetal/psu-smoke) on QEMU's xlnx-zcu102 machine, started at EL3, and check what it
# prints: startup, console, time source, MMU and caches, memory in both DDR ranges, GIC and generic-timer interrupts with the
# vector registers preserved, and the exception report. QEMU models neither the PL nor the clock registers nor cache timing.
#
# QEMU: $QEMU_SYSTEM_AARCH64, or `qemu-system-aarch64` on PATH, or the one bundled with Vitis.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
XILINX_ROOT=${XILINX_ROOT:-$HOME/AMD/2025.2}
VITIS_QEMU_DIR=$XILINX_ROOT/data/emulation/qemu/comp/qemu/sysroots/x86_64-petalinux-linux/usr
if [ -n "${QEMU_SYSTEM_AARCH64:-}" ]; then
  RUN=("$QEMU_SYSTEM_AARCH64")
elif command -v qemu-system-aarch64 >/dev/null; then
  RUN=(qemu-system-aarch64)
elif [ -x "$VITIS_QEMU_DIR/bin/qemu-system-aarch64" ]; then
  # The bundled QEMU needs its own libraries, for this process only.
  RUN=(env "LD_LIBRARY_PATH=$VITIS_QEMU_DIR/lib:$VITIS_QEMU_DIR/../lib" "$VITIS_QEMU_DIR/bin/qemu-system-aarch64")
else
  echo "no QEMU with the xlnx-zcu102 machine (set QEMU_SYSTEM_AARCH64)" >&2; exit 1
fi
cd "$ROOT/baremetal/psu-smoke"
rustup target add aarch64-unknown-none >/dev/null
cargo build --release --features qemu
OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT
# The program halts after its deliberate exception; the timeout ends QEMU.
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xlnx-zcu102,secure=on -m 4G -display none \
  -serial "file:$OUT" -serial null -monitor none -kernel target/aarch64-unknown-none/release/psu-smoke >/dev/null 2>&1 || true
cat "$OUT"
fail=0
need() { grep -q -- "$1" "$OUT" || { echo "MISSING: $1" >&2; fail=1; }; }
need "== hardroxide-psu smoke test =="
need "core 0, EL3"
need "checksums match"
need "2 MiB aligned: true"
for region in "low DDR" "top of low DDR" "high DDR" "top of high DDR"; do need "memory test, $region: .*: 0 errors"; done
need "memory test, address lines: 0 errors"
need "non-cacheable block: 0 errors"
need "timer interrupts: 20 in"
need "floating point across interrupts: 0 wrong"
need "all 32 vector registers whole across 5 interrupts: true"
need "unhandled interrupts: 0"
need "wfi woke on the timer three times"
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
[ "$fail" = 0 ] && echo "qemu UltraScale+ smoke test: ok" || { echo "qemu UltraScale+ smoke test: FAILED" >&2; exit 1; }
