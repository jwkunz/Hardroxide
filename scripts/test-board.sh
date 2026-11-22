#!/usr/bin/env bash
# Board regression: generate, build and run bare-metal and RTOS programs on a PYNQ-Z2 over JTAG
# (`hardroxide run`; needs the Vitis tools, the board in JTAG boot mode and nothing else on the
# console) and check each program's console output for its success line. No button presses or
# other manual steps. Usage: scripts/test-board.sh [name-filter]
#
#   HARDROXIDE   the hardroxide binary (default: target/debug/hardroxide, built if missing)
#   WORK         scratch directory for generated crates (default: work/board-regression)
set -uo pipefail
cd "$(dirname "$0")/.."
HRX=${HARDROXIDE:-target/debug/hardroxide}
[ -x "$HRX" ] || { cargo build -q -p hardroxide-cli && HRX=target/debug/hardroxide; }
WORK=${WORK:-work/board-regression}
FILTER=${1:-}
mkdir -p "$WORK"
pass=0; fail=0; failed=()

# gen <name> <xsa> <platform> [extra example dir...]: generate a crate, copy extra programs in.
gen() {
  local name=$1 xsa=$2 platform=$3; shift 3
  local dir=$WORK/$name
  rm -rf "$dir"
  "$HRX" generate "$xsa" --platform "$platform" -o "$dir" --name design_1 --hardroxide-path "$PWD" >/dev/null || return 1
  for extra in "$@"; do cp "$extra"/*.rs "$dir/examples/"; done
  if [ "$#" -gt 0 ] && [[ "$*" == apps/* ]]; then
    # The applications use library crates a generated crate does not depend on, and embed a WAV file.
    (cd "$dir" && for c in gfx fat net; do cargo add -q --dev --path "$OLDPWD/crates/hardroxide-$c" || exit 1; done \
      && for c in video audio; do cargo add -q --dev --path "$OLDPWD/crates/hardroxide-$c" --no-default-features || exit 1; done \
      && sed -i 's/"panic-handler"\]/"panic-handler", "net"]/' Cargo.toml && cp "$OLDPWD/apps/base/data/chime.wav" examples/) || return 1
  fi
  (cd "$dir" && cargo build -q --release --examples 2>&1) || return 1
}

# run_check <label> <crate> <example> <xsa> <seconds> <expected text>
run_check() {
  local label=$1 name=$2 example=$3 xsa=$4 secs=$5 expect=$6
  [ -n "$FILTER" ] && [[ "$label" != *$FILTER* ]] && return
  local out
  out=$(cd "$WORK/$name" && timeout $((secs + 60)) "$OLDPWD/$HRX" run "target/armv7a-none-eabihf/release/examples/$example" --xsa "$OLDPWD/$xsa" --watch "$secs" 2>&1)
  if grep -qF -- "$expect" <<<"$out"; then
    echo "PASS  $label"; pass=$((pass + 1))
  else
    echo "FAIL  $label (expected: $expect)"; echo "$out" | tail -8 | sed 's/^/      /'
    fail=$((fail + 1)); failed+=("$label")
  fi
}

B=fixtures/board_tests_bm
gen gpio-bm fixtures/board/pynq_z2_gpio.xsa baremetal "$B/baremetal_gpio" && {
  run_check "baremetal/gpio board_check" gpio-bm board_check fixtures/board/pynq_z2_gpio.xsa 8 "all checks passed"
}
gen gpio-rt fixtures/board/pynq_z2_gpio.xsa rtos "$B/rtos_gpio" && {
  run_check "rtos/gpio control loop" gpio-rt control fixtures/board/pynq_z2_gpio.xsa 8 "worst deviation from the 10 ms period"
}
gen periph fixtures/board/pynq_z2_peripherals.xsa baremetal "$B/baremetal_timer" && {
  run_check "baremetal/timer_irq" periph timer_irq fixtures/board/pynq_z2_peripherals.xsa 8 "round 2: event Some(3)"
  run_check "baremetal/timer_ticks" periph timer_ticks fixtures/board/pynq_z2_peripherals.xsa 8 "gap between events"
}
gen dma fixtures/board/pynq_z2_dma_loopback.xsa baremetal "$B/baremetal_dma" && {
  run_check "baremetal/dma_loopback" dma dma_loopback fixtures/board/pynq_z2_dma_loopback.xsa 8 "DMA loopback OK"
  run_check "baremetal/dma_speed" dma dma_speed fixtures/board/pynq_z2_dma_loopback.xsa 8 "0 mismatches"
}
gen base-bm fixtures/board/pynq_base_overlay.xsa baremetal apps/base/baremetal && {
  run_check "base/board_check" base-bm board_check fixtures/board/pynq_base_overlay.xsa 8 "all checks passed"
  run_check "base/selftest" base-bm selftest fixtures/board/pynq_base_overlay.xsa 10 "SELFTEST OK"
}
gen base-rt fixtures/board/pynq_base_overlay.xsa rtos apps/base/rtos && {
  run_check "base/rt_irq_latency" base-rt rt_irq_latency fixtures/board/pynq_base_overlay.xsa 20 "LATENCY OK"
  run_check "base/rt_lights" base-rt rt_lights fixtures/board/pynq_base_overlay.xsa 8 "mood light"
}

# The video programs need a monitor on HDMI OUT to be seen; this only checks that the pipeline comes up and the
# program runs to its end (a hang or a DMA error shows as a missing line).
gen base-video fixtures/board/pynq_base_overlay.xsa baremetal apps/base/baremetal && {
  (cd "$WORK/base-video" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-gfx" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-video" --no-default-features && cargo build -q --release --examples 2>&1) \
    && run_check "base/video_demo (starts and runs)" base-video video_demo fixtures/board/pynq_base_overlay.xsa 30 "animated frames in"
}

gen base-rtvideo fixtures/board/pynq_base_overlay.xsa rtos apps/base/rtos && {
  (cd "$WORK/base-rtvideo" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-gfx" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-video" --no-default-features && cargo build -q --release --examples 2>&1) \
    && run_check "base/rt_video (60 fps by interrupt)" base-rtvideo rt_video fixtures/board/pynq_base_overlay.xsa 12 "frame interval worst deviation"
}

# Audio (and, with HRX_SD_SERIAL set, the SD card programs): plays a short tune (headphones optional) and checks the codec comes up; with HRX_AUDIO_LOOPBACK=1 and a cable
# from HPH OUT to LINE IN it also plays and records a 1 kHz tone.
gen base-audio fixtures/board/pynq_base_overlay.xsa baremetal apps/base/baremetal && {
  (cd "$WORK/base-audio" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-gfx" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-video" --no-default-features && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-audio" --no-default-features && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-fat" && cargo add -q --dev --path "$OLDPWD/crates/hardroxide-net" && sed -i 's/"panic-handler"\]/"panic-handler", "net"]/' Cargo.toml && cp "$OLDPWD/apps/base/data/chime.wav" examples/ && cargo build -q --release --examples 2>&1) && {
    run_check "base/audio_tone (codec up, no missed samples)" base-audio audio_tone fixtures/board/pynq_base_overlay.xsa 14 "chord: 58593 frames, 0 missed"
    run_check "base/audio_wav" base-audio audio_wav fixtures/board/pynq_base_overlay.xsa 14 "pass 2: 97661 frames played, 0 missed"
    if [ "${HRX_AUDIO_LOOPBACK:-0}" = 1 ]; then
      run_check "base/audio_loopback_test (cable HPH OUT to LINE IN)" base-audio audio_loopback_test fixtures/board/pynq_base_overlay.xsa 12 "LOOPBACK OK"
      run_check "base/audio_record_test (cable HPH OUT to LINE IN)" base-audio audio_record_test fixtures/board/pynq_base_overlay.xsa 14 "RECORD/PLAYBACK OK"
    fi
    # Network: needs the Ethernet cable to a PC (or a router) and, for the web server, `curl`; set HRX_NET=1. The address comes
    # from DHCP or is 192.168.2.100 after eight seconds (a PC port at 192.168.2.1/24).
    if [ "${HRX_NET:-0}" = 1 ]; then
      run_check "base/net_ping (link, DHCP or fallback address)" base-audio net_ping fixtures/board/pynq_base_overlay.xsa 40 "NET PING READY"
      run_check "base/net_web (link, address, server up)" base-audio net_web fixtures/board/pynq_base_overlay.xsa 40 "NET WEB READY"
    fi
    # SD card: ERASES the card in the slot, so it runs only when HRX_SD_SERIAL names that card (the serial number
    # `sd_info` prints, for example 0x2df43e3b); the format test refuses any other card that holds data.
    if [ -n "${HRX_SD_SERIAL:-}" ]; then
      (cd "$WORK/base-audio" && HRX_ERASE_SERIAL="$HRX_SD_SERIAL" cargo build -q --release --example sd_erase_and_test 2>&1) &&
        run_check "base/sd_erase_and_test (formats the card!)" base-audio sd_erase_and_test fixtures/board/pynq_base_overlay.xsa 150 "SD TEST OK"
      run_check "base/sd_logger" base-audio sd_logger fixtures/board/pynq_base_overlay.xsa 25 "SD LOGGER OK"
      run_check "base/sd_wav" base-audio sd_wav fixtures/board/pynq_base_overlay.xsa 30 "SD WAV OK"
      run_check "base/sd_bmp (HDMI)" base-audio sd_bmp fixtures/board/pynq_base_overlay.xsa 40 "SD BMP OK"
    fi
  }
}

echo "board regression: $pass passed, $fail failed"
[ "$fail" = 0 ] || { printf 'failed: %s\n' "${failed[@]}"; exit 1; }
