#!/usr/bin/env bash
# The board tests of the Zynq UltraScale+ support, on an RFSoC 4x2 in JTAG boot mode connected by USB (needs Vitis).
# Volatile: nothing is written to storage. Runs the smoke program (processor system and PL interrupts) with the loopback
# fixture in the PL, once more without programming the PL (`run --no-pl`), and the kernel's test (baremetal/rt-smoke).
# The two-hour jitter benchmark (baremetal/rt-jitter) is run by hand.
# HRX_SD_SERIAL=0x... adds the SD card test, which ERASES that card (and refuses any other).
# HRX_NET=1 adds the Ethernet test (the board cabled to this computer, which serves DHCP on that link).
#
#   scripts/test-board-rfsoc.sh                 HRX_TTY: the console port (default /dev/ttyUSB1)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TTY=${HRX_TTY:-/dev/ttyUSB1}
XSA=$ROOT/fixtures/rfsoc4x2/rfsoc4x2_loopback.xsa
cargo build -q -p hardroxide-cli
OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT
fail=0
run() { # <features> <extra run arguments...>
  local features=$1; shift
  (cd baremetal/psu-smoke && cargo build -q --release --features "$features")
  target/debug/hardroxide run baremetal/psu-smoke/target/aarch64-unknown-none/release/psu-smoke \
    --xsa "$XSA" --tty "$TTY" --watch 20 "$@" 2>&1 | tr -d '\r' | tee "$OUT"
}
need() { grep -q -- "$1" "$OUT" || { echo "MISSING: $1" >&2; fail=1; }; }
processor() {
  need "cpu 1199999988 Hz, counter 33333333 Hz, core 0, EL3"
  need "checksums match"
  for region in "low DDR" "top of low DDR" "high DDR" "top of high DDR"; do need "memory test, $region: 64 MiB .*: 0 errors"; done
  need "memory test, address lines: 0 errors"
  need "non-cacheable block: 0 errors"
  need "timer interrupts: 20 in 200"
  need "floating point across interrupts: 0 wrong"
  need "all 32 vector registers whole across 5 interrupts: true"
  need "ALL OK"
  need "EXCEPTION: undefined instruction"
}

echo "== processor system and PL"
run pl
processor
need "PL block RAM, 64 KiB: 0 errors"
need "PL GPIO loop: wrote 0xa5, read 0xa5"
need "PL GPIO interrupt (ID 127), handler: 10 of 10"
need "PL timer interrupt (ID 121), waited for: 3 of 3; nothing before the timer ran: true"

echo "== without programming the PL"
run "" --no-pl
processor

echo "== the kernel"
(cd baremetal/rt-smoke && cargo build -q --release --target aarch64-unknown-none)
target/debug/hardroxide run baremetal/rt-smoke/target/aarch64-unknown-none/release/rt-smoke \
  --xsa "$XSA" --tty "$TTY" --watch 15 2>&1 | tr -d '\r' | tee "$OUT"
need "a take that would wait false"
need "main went on and no task ran"
need "no drift"
need "10/10 items received in order"
need "woke the task: true, after 20 ticks"
need "20 wakeups of a 5 ms sleeper over a spinning task, 0 late"
need "inheritance worked"
need "second (after rearm) true, nothing pending true"
need "0 broken (preserved)"
need "RT ALL OK"
need "EXCEPTION: undefined instruction"
# The SD card: only with HRX_SD_SERIAL naming the card in the slot, WHICH IS ERASED (baremetal/sd-info prints the serial
# number without writing). The start of what the board wrote comes back over JTAG and is checked with fsck.fat.
if [ -n "${HRX_SD_SERIAL:-}" ]; then
  echo "== the SD card ($HRX_SD_SERIAL is erased)"
  (cd baremetal/sd-smoke && HRX_ERASE_SERIAL=$HRX_SD_SERIAL cargo build -q --release --target aarch64-unknown-none)
  target/debug/hardroxide run baremetal/sd-smoke/target/aarch64-unknown-none/release/sd-smoke \
    --xsa "$XSA" --tty "$TTY" --watch 40 2>&1 | tr -d '\r' | tee "$OUT"
  need "raw blocks: 4 blocks written and read back"
  need "formatted and mounted"
  need "SD SMOKE OK"
  if grep -q "^DUMP " "$OUT"; then
    read -r _ at len < <(grep "^DUMP " "$OUT")
    DUMP=$(mktemp -d)
    printf 'connect\ntargets -set -nocase -filter {name =~ "PSU"}\nmrd -force -bin -file {%s} %s %d\nexit\n' \
      "$DUMP/card.bin" "$at" $((len / 4)) >"$DUMP/dump.tcl"
    VITIS=${VITIS_SETTINGS:-$HOME/AMD/2025.2/Vitis/settings64.sh}
    bash -c "set +u; source '$VITIS'; xsdb '$DUMP/dump.tcl'" >/dev/null
    # The partition (from sector 2048) at its full size, sparse beyond what was fetched.
    python3 - "$DUMP/card.bin" "$DUMP/part.img" <<'PY'
import struct, sys
d = open(sys.argv[1], "rb").read()
start, count = struct.unpack_from("<II", d, 446 + 8)
with open(sys.argv[2], "wb") as f:
    f.write(d[start * 512:])
    f.truncate(count * 512)
PY
    fsck.fat -n "$DUMP/part.img" || { echo "fsck.fat found errors" >&2; fail=1; }
    rm -rf "$DUMP"
  else
    echo "MISSING: DUMP" >&2; fail=1
  fi
fi

# Ethernet: only with HRX_NET=1 and a cable from the board to this computer (which must give it an address by DHCP).
if [ -n "${HRX_NET:-}" ]; then
  echo "== Ethernet"
  (cd baremetal/net-smoke && cargo build -q --release --target aarch64-unknown-none)
  : >"$OUT"
  target/debug/hardroxide run baremetal/net-smoke/target/aarch64-unknown-none/release/net-smoke \
    --xsa "$XSA" --tty "$TTY" --watch 240 >"$OUT" 2>&1 &
  RUN_PID=$!
  for _ in $(seq 1 120); do grep -q "NET READY\|NET SMOKE FAILED" "$OUT" && break; sleep 0.5; done
  tr -d '\r' <"$OUT"
  need "DP83867, clock delays set"
  need "NET READY ip=.* dhcp=true"
  IP=$(sed -n 's/.*NET READY ip=\([0-9.]*\) dhcp=true.*/\1/p' "$OUT" | head -1)
  if [ -n "$IP" ]; then
    ping -c 5 -i 0.2 -W 1 "$IP" | tail -3 || fail=1
    python3 scripts/net_check.py "$IP" 80 9 443 7 --figures || fail=1
  else
    fail=1
  fi
  # Stop showing the console (either may find nothing left to stop).
  pkill -P "$RUN_PID" 2>/dev/null || true
  kill "$RUN_PID" 2>/dev/null || true
  wait "$RUN_PID" 2>/dev/null || true
fi

[ "$fail" = 0 ] && echo "RFSoC 4x2 board test: ok" || { echo "RFSoC 4x2 board test: FAILED" >&2; exit 1; }
