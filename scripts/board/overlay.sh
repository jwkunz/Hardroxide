#!/bin/sh
# Run ON THE BOARD, as root: apply a generated interrupts.dtso so each PL
# interrupt appears as /dev/uioN. Replaces an overlay applied earlier by this script.
#
#   sudo ./overlay.sh my_design/interrupts.dtso
#
# Caveat: a GIC interrupt line keeps the trigger type (level/edge) of the first
# overlay that mapped it until reboot. If a UIO device later returns EIO, reboot
# before applying an overlay that needs the other type on the same line.
set -eu
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
src=${1:?usage: overlay.sh interrupts.dtso}
command -v dtc >/dev/null || { echo "dtc (device-tree-compiler) is not installed" >&2; exit 1; }
O=/sys/kernel/config/device-tree/overlays
mkdir -p "$O" 2>/dev/null || true
if [ -d "$O/hrx" ]; then
  D=/sys/bus/platform/drivers/uio_pdrv_genirq
  for d in "$D"/*; do
    case $(basename "$d") in bind|unbind|uevent|module) continue ;; esac
    [ -e "$d" ] && echo "$(basename "$d")" > "$D/unbind" 2>/dev/null || true
  done
  rmdir "$O/hrx"
fi
tmp=$(mktemp)
dtc -@ -I dts -O dtb -o "$tmp" "$src" 2>/dev/null
mkdir "$O/hrx"
cat "$tmp" > "$O/hrx/dtbo"
rm -f "$tmp"
echo "overlay status: $(cat "$O/hrx/status")"
for u in /sys/class/uio/uio*; do [ -e "$u" ] && echo "$(basename "$u"): $(cat "$u/name")"; done
true
