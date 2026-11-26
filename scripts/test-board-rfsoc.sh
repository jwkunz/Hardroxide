#!/usr/bin/env bash
# The board tests of the Zynq UltraScale+ support, on an RFSoC 4x2 in JTAG boot mode connected by USB (needs Vitis).
# Volatile: nothing is written to storage. Runs the smoke program (processor system and PL interrupts) with the loopback
# fixture in the PL, once more without programming the PL (`run --no-pl`), and then everything a student would run, on the
# base overlay: the smoke program again, the overlay's example (apps/rfsoc4x2/baremetal/lights), the kernel's test
# (baremetal/rt-smoke), a read-only look at the I2C buses and power monitors (baremetal/i2c-info), and the board's own
# hardware (baremetal/rfsoc-board: identity, temperatures, power monitors, OLED, clock chips, LEDs; it programs the clock
# chips, which forget at power-off), and the overlay and the board hardware on the kernel (apps/rfsoc4x2/rtos/rt_lights,
# baremetal/rt-board). What the display shows, the buttons and the switches are for a person to check.
# HRX_SD_SERIAL=0x... adds the SD card test, which ERASES that card (and refuses any other).
# HRX_NET=1 adds the Ethernet tests (bare metal, and the web page on the kernel; the board cabled to this computer,
# which serves DHCP on that link).
#
#   scripts/test-board-rfsoc.sh                 HRX_TTY: the console port (default /dev/ttyUSB1)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TTY=${HRX_TTY:-/dev/ttyUSB1}
# The base overlay for what a student runs; the loopback fixture, which wires blocks to each other, for the driver tests.
XSA=$ROOT/fixtures/board/rfsoc4x2_base_overlay.xsa
LOOPBACK=$ROOT/fixtures/rfsoc4x2/rfsoc4x2_loopback.xsa
cargo build -q -p hardroxide-cli
OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT
fail=0
run() { # <features> <xsa> <extra run arguments...>
  local features=$1 xsa=$2; shift 2
  (cd baremetal/psu-smoke && cargo build -q --release --features "$features")
  target/debug/hardroxide run baremetal/psu-smoke/target/aarch64-unknown-none/release/psu-smoke \
    --xsa "$xsa" --tty "$TTY" --watch 20 "$@" 2>&1 | tr -d '\r' | tee "$OUT"
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
run pl "$LOOPBACK"
processor
need "PL block RAM, 64 KiB: 0 errors"
need "PL GPIO loop: wrote 0xa5, read 0xa5"
need "PL GPIO interrupt (ID 127), handler: 10 of 10"
need "PL timer interrupt (ID 121), waited for: 3 of 3; nothing before the timer ran: true"

echo "== without programming the PL"
run "" "$LOOPBACK" --no-pl
processor

echo "== processor system, with the base overlay"
run "" "$XSA"
processor

# The base overlay's applications (apps/rfsoc4x2), each built against a freshly generated crate, as a student does.
GEN=$ROOT/target/rfsoc4x2/generated
application() { # <platform> <application> <seconds to watch>
  local platform=$1 app=$2 dir=$GEN/apps-$1
  rm -rf "$dir"
  target/debug/hardroxide generate "$XSA" --platform "$platform" --name design_1 -o "$dir" --hardroxide-path "$ROOT" >/dev/null
  cp "apps/rfsoc4x2/$platform/$app.rs" "$dir/examples/"
  printf '\n[workspace]\n' >>"$dir/Cargo.toml"
  (cd "$dir" && CARGO_TARGET_DIR="$GEN/target-apps-$platform" cargo build -q --release --example "$app")
  target/debug/hardroxide run "$GEN/target-apps-$platform/aarch64-unknown-none/release/examples/$app" \
    --xsa "$XSA" --tty "$TTY" --watch "$3" 2>&1 | tr -d '\r' | tee "$OUT"
}

echo "== the base overlay: lights"
application baremetal lights 10
need "FPGA temperature: [0-9]* C"
need "buttons interrupt: quiet before true, arrived true, cleared true"
need "switches interrupt: quiet before true, arrived true, cleared true"
need "SELF TEST OK"

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
echo "== I2C (read only)"
(cd baremetal/i2c-info && cargo build -q --release)
target/debug/hardroxide run baremetal/i2c-info/target/aarch64-unknown-none/release/i2c-info \
  --xsa "$XSA" --tty "$TTY" --watch 12 2>&1 | tr -d '\r' | tee "$OUT"
need "bus 0 at .*: 0x40 0x41 0x42 0x43 0x44 0x45 0x46 0x47 0x48  (9 found, 0 other errors)"
need "bus 1 at .*0x0d.*0x50 0x58 .*0 other errors"
need "9 of 9 monitors read"
need "I2C INFO DONE"

echo "== the board's hardware"
(cd baremetal/rfsoc-board && cargo build -q --release)
target/debug/hardroxide run baremetal/rfsoc-board/target/aarch64-unknown-none/release/rfsoc-board \
  --xsa "$XSA" --tty "$TTY" --watch 12 2>&1 | tr -d '\r' | tee "$OUT"
need "Ethernet address [0-9a-f:]\{17\}"
need "VCC0V85 .* mV"
need "address counter read back: Some(10), Some(50), Some(25)"
need "LMX2594 (DAC): Unlocked"
need "LMX2594 (DAC): Locked"
need "BOARD DEMO OK"

echo "== the kernel: a mood light on the base overlay"
application rtos rt_lights 14
need "buttons interrupt woke the buttons task: true"
need "switches interrupt woke the buttons task: true"
need "RT LIGHTS OK"

echo "== the kernel: the board's hardware"
(cd baremetal/rt-board && cargo build -q --release)
target/debug/hardroxide run baremetal/rt-board/target/aarch64-unknown-none/release/rt-board \
  --xsa "$XSA" --tty "$TTY" --watch 10 2>&1 | tr -d '\r' | tee "$OUT"
need "clocks: programmed, both synthesizers locked: true"
need "monitor: [0-9]* updates (0 incomplete)"
need "follows: true"
need "RT BOARD OK"

# The generated crates: each design's crate runs its board check and its driver test (fixtures/board_tests_rfsoc), and the
# base overlay's RTOS crate its control loop.
generated() { # <design> <xsa> <program> <last line expected>
  local name=$1 xsa=$2 program=$3 last=$4 dir=$GEN/$1
  rm -rf "$dir"
  target/debug/hardroxide generate "$xsa" --platform baremetal --name design_1 -o "$dir" --hardroxide-path "$ROOT" >/dev/null
  cp "fixtures/board_tests_rfsoc/$name/$program.rs" "$dir/examples/"
  printf '\n[workspace]\n' >>"$dir/Cargo.toml"
  for example in board_check "$program"; do
    (cd "$dir" && CARGO_TARGET_DIR="$GEN/target-$name" cargo build -q --release --example "$example")
    target/debug/hardroxide run "$GEN/target-$name/aarch64-unknown-none/release/examples/$example" \
      --xsa "$xsa" --tty "$TTY" --watch 8 2>&1 | tr -d '\r' | tee "$OUT"
    if [ "$example" = board_check ]; then need "all checks passed"; else need "^$last\$"; fi
  done
}
echo "== generated crates"
for d in gpio:gpio_check:"GPIO OK" intc:controller:"INTC OK" loopback:drivers:"LOOPBACK OK" dma:both:"DMA OK" \
  hls_vadd:run:"HLS VADD OK" hls_scale:run:"HLS SCALE OK"; do
  IFS=: read -r name program last <<<"$d"
  generated "$name" "$ROOT/fixtures/rfsoc4x2/rfsoc4x2_$name.xsa" "$program" "$last"
done
generated base "$XSA" tour "BASE OK"
echo "== the base overlay's generated RTOS crate"
rm -rf "$GEN/base-rtos"
target/debug/hardroxide generate "$XSA" --platform rtos -o "$GEN/base-rtos" --hardroxide-path "$ROOT" >/dev/null
printf '\n[workspace]\n' >>"$GEN/base-rtos/Cargo.toml"
(cd "$GEN/base-rtos" && CARGO_TARGET_DIR="$GEN/target-base-rtos" cargo build -q --release --example rt_control)
target/debug/hardroxide run "$GEN/target-base-rtos/aarch64-unknown-none/release/examples/rt_control" \
  --xsa "$XSA" --tty "$TTY" --watch 6 2>&1 | tr -d '\r' | tee "$OUT"
need "control loop: [0-9]* releases, worst deviation from 10 ms: [0-9] us"

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

# Stop showing the console of the program run in the background (either may find nothing left to stop).
stop_console() {
  pkill -P "$RUN_PID" 2>/dev/null || true
  kill "$RUN_PID" 2>/dev/null || true
  wait "$RUN_PID" 2>/dev/null || true
}

# Ethernet: only with HRX_NET=1 and a cable from the board to this computer (which must give it an address by DHCP).
if [ -n "${HRX_NET:-}" ]; then
  echo "== Ethernet"
  (cd baremetal/net-smoke && cargo build -q --release --target aarch64-unknown-none)
  : >"$OUT"
  target/debug/hardroxide run baremetal/net-smoke/target/aarch64-unknown-none/release/net-smoke \
    --xsa "$XSA" --tty "$TTY" --watch 240 >"$OUT" 2>&1 &
  RUN_PID=$!
  for _ in $(seq 1 240); do grep -q "NET READY\|NET SMOKE FAILED" "$OUT" && break; sleep 0.5; done
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
  stop_console

  echo "== Ethernet on the kernel: the base overlay's web page beside its control loop"
  dir=$GEN/apps-network
  rm -rf "$dir"
  target/debug/hardroxide generate "$XSA" --platform rtos --name design_1 -o "$dir" --hardroxide-path "$ROOT" >/dev/null
  cp apps/rfsoc4x2/rtos/rt_dashboard.rs "$dir/examples/"
  # The lines its tutorial has a student add.
  sed -i "s|^\[dev-dependencies\]\$|&\nhardroxide-net = { path = \"$ROOT/crates/hardroxide-net\" }\nhardroxide-psu = { path = \"$ROOT/crates/hardroxide-psu\", default-features = false, features = [\"net\"] }|" "$dir/Cargo.toml"
  printf '\n[workspace]\n' >>"$dir/Cargo.toml"
  (cd "$dir" && CARGO_TARGET_DIR="$GEN/target-apps-rtos" cargo build -q --release --example rt_dashboard)
  : >"$OUT"
  target/debug/hardroxide run "$GEN/target-apps-rtos/aarch64-unknown-none/release/examples/rt_dashboard" \
    --xsa "$XSA" --tty "$TTY" --watch 60 >"$OUT" 2>&1 &
  RUN_PID=$!
  for _ in $(seq 1 160); do grep -q "DASHBOARD READY\|no link" "$OUT" && break; sleep 0.5; done
  tr -d '\r' <"$OUT"
  IP=$(sed -n 's/.*DASHBOARD READY: http:\/\/\([0-9.]*\)\/.*/\1/p' "$OUT" | head -1)
  if [ -n "$IP" ]; then
    curl -sf -m 4 -o /dev/null "http://$IP/" || { echo "MISSING: the page" >&2; fail=1; }
    curl -sf -m 4 -X POST "http://$IP/colour?value=2" || { echo "MISSING: setting a colour" >&2; fail=1; }
    answered=0
    for _ in $(seq 1 500); do curl -sf -m 3 -o /dev/null "http://$IP/state" && answered=$((answered + 1)); done
    state=$(curl -s -m 4 "http://$IP/state")
    echo "$answered of 500 requests answered; $state"
    [ "$answered" = 500 ] || { echo "MISSING: an answer to every request" >&2; fail=1; }
    grep -q '"colour":2,' <<<"$state" || { echo "MISSING: the chosen colour" >&2; fail=1; }
    grep -q '"worst_us":[0-9],' <<<"$state" || { echo "MISSING: the control loop within 10 us of its period" >&2; fail=1; }
    curl -sf -m 4 -X POST "http://$IP/colour?value=0" || true
  else
    echo "MISSING: DASHBOARD READY" >&2; fail=1
  fi
  stop_console
fi

[ "$fail" = 0 ] && echo "RFSoC 4x2 board test: ok" || { echo "RFSoC 4x2 board test: FAILED" >&2; exit 1; }
