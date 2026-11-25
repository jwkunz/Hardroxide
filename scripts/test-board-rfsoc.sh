#!/usr/bin/env bash
# The board tests of the Zynq UltraScale+ support, on an RFSoC 4x2 in JTAG boot mode connected by USB (needs Vitis).
# Volatile: nothing is written to storage. Runs the smoke program (processor system and PL interrupts) with the loopback
# fixture in the PL, then once more without programming the PL (`run --no-pl`).
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
[ "$fail" = 0 ] && echo "RFSoC 4x2 board test: ok" || { echo "RFSoC 4x2 board test: FAILED" >&2; exit 1; }
