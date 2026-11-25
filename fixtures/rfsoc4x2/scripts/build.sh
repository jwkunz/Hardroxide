#!/usr/bin/env bash
# Builds RFSoC 4x2 fixtures: scripts/build.sh gpio dma ...   -> ../rfsoc4x2_<name>.xsa (replaced only on success)
# Needs Vivado (and, for hls_*, vitis-run) with a license for the xczu48dr. VIVADO_SETTINGS names the tools' settings64.sh
# (default ~/AMD/2025.2/Vivado/settings64.sh); HRX_WORK the build directory (default: target/rfsoc4x2 in the repository).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
settings=${VIVADO_SETTINGS:-$HOME/AMD/2025.2/Vivado/settings64.sh}
work=${HRX_WORK:-$here/../../../target/rfsoc4x2}
mkdir -p "$work"
work=$(cd "$work" && pwd)
# shellcheck disable=SC1090
set +u; source "$settings"; set -u
[ $# -gt 0 ] || { sed -n '2,4p' "$0"; exit 2; }
for name in "$@"; do
    extra=()
    case $name in
    hls_*)
        kernel=${name#hls_}
        if [ ! -d "$work/hls/${kernel}prj/sol/impl/ip" ]; then
            mkdir -p "$work/hls"
            (cd "$work/hls" && vitis-run --mode hls --tcl "$here/build_hls_ip.tcl" --work_dir "$work/hls/work" > "$work/hls.log" 2>&1) ||
                { echo "HLS failed, see $work/hls.log"; exit 1; }
        fi
        extra=("$work/hls/${kernel}prj/sol/impl/ip")
        ;;
    esac
    rm -rf "$work/$name" "$work/$name".*.xdc
    mkdir -p "$work/$name" "$work/out"
    rm -f "$work/out/rfsoc4x2_$name.xsa"
    echo "building $name (log: $work/$name.log)"
    (cd "$work/$name" && vivado -mode batch -notrace -nojournal -log "$work/$name.log" -source "$here/build_$name.tcl" \
        -tclargs "$work/$name/project" "$work/out/rfsoc4x2_$name.xsa" "${extra[@]}" > /dev/null 2>&1) &&
        [ -f "$work/out/rfsoc4x2_$name.xsa" ] ||
        { echo "$name FAILED (log: $work/$name.log):"; grep -E "^(ERROR|CRITICAL)" "$work/$name.log" | head -20; tail -5 "$work/$name.log"; exit 1; }
    mv "$work/out/rfsoc4x2_$name.xsa" "$here/../rfsoc4x2_$name.xsa"
    grep "^hrx: timing" "$work/$name.log"
    echo "wrote fixtures/rfsoc4x2/rfsoc4x2_$name.xsa"
done
