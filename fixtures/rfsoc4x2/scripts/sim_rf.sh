#!/usr/bin/env bash
# Simulates the RF loopback design's logic (../hdl) with the Vivado simulator. Needs no license.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
settings=${VIVADO_SETTINGS:-$HOME/AMD/2025.2/Vivado/settings64.sh}
work=${HRX_WORK:-$here/../../../target/rfsoc4x2}/sim
rm -rf "$work"; mkdir -p "$work"; cd "$work"
set +u; source "$settings"; set -u
xvlog "$here"/../hdl/hrx_player.v "$here"/../hdl/hrx_capture.v "$here"/../hdl/tb_rf.v "$XILINX_VIVADO/data/verilog/src/glbl.v" > xvlog.log
for latency in 1 2; do # both memory latencies the player supports
    xelab -L xpm -L unisims_ver --generic_top "MEM_LATENCY=$latency" tb_rf glbl -s tb$latency > xelab$latency.log || { cat xelab$latency.log; exit 1; }
    xsim tb$latency -R > xsim$latency.log
    echo "memory latency $latency: $(grep '^TB' xsim$latency.log | head -5)"
    grep -q "^TB PASS" xsim$latency.log
done
