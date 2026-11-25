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
need2 "second (after rearm) true, nothing pending true"
need2 "a take that would wait false"
need2 "main went on and no task ran"
need2 "0 broken (preserved)"
need2 "RT ALL OK"
need2 "EXCEPTION: undefined instruction"
[ "$fail" = 0 ] && echo "qemu kernel test: ok" || { echo "qemu kernel test: FAILED" >&2; exit 1; }

# ---- the SD host driver and the FAT32 file system (baremetal/sd-smoke) ----
# QEMU's Zynq machine has an SD host controller; the "card" is a throwaway image the program formats,
# fills and reads back. Afterwards the host's fsck.fat checks the file system the program wrote.
cd "$ROOT/baremetal/sd-smoke"
cargo build --release --features qemu
IMG=$(mktemp)
OUT3=$(mktemp)
trap 'rm -f "$OUT" "$OUT2" "$OUT3" "$IMG" "$IMG.part"' EXIT
truncate -s 128M "$IMG"
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none \
  -serial stdio -serial null -monitor none -drive "if=sd,index=0,file=$IMG,format=raw" \
  -kernel target/armv7a-none-eabihf/release/sd-smoke >"$OUT3" 2>&1 || true
cat "$OUT3"
fail=0
need3() { grep -q -- "$1" "$OUT3" || { echo "MISSING: $1" >&2; fail=1; }; }
need3 "raw blocks: 4 blocks written and read back"
need3 "formatted and mounted"
need3 "SD SMOKE OK"
if command -v fsck.fat >/dev/null; then
  # The partition starts at sector 2048 (1 MiB).
  dd if="$IMG" of="$IMG.part" bs=512 skip=2048 status=none
  if fsck.fat -n "$IMG.part" >/dev/null; then echo "fsck.fat on the card image: clean"; else echo "fsck.fat found problems" >&2; fail=1; fi
else
  echo "(fsck.fat not installed; skipping the file system check)"
fi
[ "$fail" = 0 ] && echo "qemu SD test: ok" || { echo "qemu SD test: FAILED" >&2; exit 1; }

# ---- the Ethernet MAC driver and the network stack (baremetal/net-smoke) ----
# QEMU's Zynq machine has the Cadence GEM and user-mode networking (a virtual router that answers DHCP and forwards host ports to the
# guest). The program gets an address over DHCP and serves HTTP (80), a TCP echo (9) and a UDP echo (7); the host talks to it with curl
# and Python sockets, so the stack is checked against independent implementations of the protocols.
cd "$ROOT/baremetal/net-smoke"
cargo build --release --features qemu
OUT4=$(mktemp)
trap 'rm -f "$OUT" "$OUT2" "$OUT3" "$OUT4" "$IMG" "$IMG.part"' EXIT
"${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none -serial stdio -serial null -monitor none \
  -nic "user,model=cadence_gem,mac=02:48:52:58:00:01,hostfwd=tcp:127.0.0.1:18080-:80,hostfwd=tcp:127.0.0.1:18009-:9,hostfwd=tcp:127.0.0.1:18443-:443,hostfwd=udp:127.0.0.1:18007-:7" \
  -kernel target/armv7a-none-eabihf/release/net-smoke >"$OUT4" 2>&1 &
QPID=$!
fail=0
for _ in $(seq 1 100); do grep -q "NET READY" "$OUT4" && break; sleep 0.1; done
cat "$OUT4"
grep -q "NET READY ip=10.0.2.15 dhcp=true" "$OUT4" || { echo "MISSING: an address from DHCP" >&2; fail=1; }
if [ "$fail" = 0 ]; then
python3 "$ROOT/scripts/net_check.py" 127.0.0.1 18080 18009 18443 18007 || fail=1
fi
kill "$QPID" 2>/dev/null; wait "$QPID" 2>/dev/null || true
[ "$fail" = 0 ] && echo "qemu network test: ok" || { echo "qemu network test: FAILED" >&2; exit 1; }

# ---- Zynq UltraScale+ (hardroxide-psu), on QEMU's AArch64 machine ----
"$ROOT/scripts/test-qemu-psu.sh"
