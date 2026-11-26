#!/bin/sh
# Run ON THE BOARD, as root: apply a generated interrupts.dtso so each PL
# interrupt appears as /dev/uioN. Replaces an overlay applied earlier by this script.
#
#   sudo ./overlay.sh my_design/interrupts.dtso
#
# A compiled overlay (.dtbo) is applied as it is, for images without dtc (as the
# RFSoC 4x2's): compile it elsewhere, `dtc -@ -I dts -O dtb -o interrupts.dtbo interrupts.dtso`.
#
# Caveat: a GIC interrupt line keeps the trigger type (level/edge) of the first
# overlay that mapped it until reboot. If a UIO device later returns EIO, reboot
# before applying an overlay that needs the other type on the same line.
set -eu
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
src=${1:?usage: overlay.sh interrupts.dtso}
case $src in
  *.dtbo) [ -f "$src" ] || { echo "no such file: $src" >&2; exit 1; } ;;
  *) command -v dtc >/dev/null || { echo "dtc (device-tree-compiler) is not installed; pass a compiled .dtbo" >&2; exit 1; } ;;
esac
O=/sys/kernel/config/device-tree/overlays
mkdir -p "$O" 2>/dev/null || true
if [ -d "$O/hrx" ]; then
  D=/sys/bus/platform/drivers/uio_pdrv_genirq
  for d in "$D"/*; do
    # `*.fabric` is the image's own device (on pl_ps_irq0[0] of an RFSoC 4x2), not one of ours: the generated crate
    # uses it, and unbinding it would take it away until the next boot.
    case $(basename "$d") in bind|unbind|uevent|module|*.fabric) continue ;; esac
    [ -e "$d" ] && echo "$(basename "$d")" > "$D/unbind" 2>/dev/null || true
  done
  rmdir "$O/hrx"
fi
tmp=$(mktemp)
case $src in
  *.dtbo) cat "$src" > "$tmp" ;;
  *) dtc -@ -I dts -O dtb -o "$tmp" "$src" 2>/dev/null ;;
esac
# The PYNQ image of a Zynq-7000 board has its own UIO device, `40000000.fabric`, holding IRQ_F2P[0] (SPI cell 29), and a
# line can be claimed once: release it if this overlay wants that line (until the next boot). A generated crate for
# Zynq UltraScale+ uses its image's device instead, and its overlay leaves that line out.
F=/sys/bus/platform/drivers/uio_pdrv_genirq
case $src in
  *.dtso)
    if [ -e "$F/40000000.fabric" ] && grep -q 'interrupts = <0 29 ' "$src"; then
      echo 40000000.fabric > "$F/unbind"
      echo "released the image's 40000000.fabric device, which held IRQ_F2P[0] (a reboot restores it)"
    fi
    ;;
esac
mkdir "$O/hrx"
cat "$tmp" > "$O/hrx/dtbo"
rm -f "$tmp"
echo "overlay status: $(cat "$O/hrx/status")"
for u in /sys/class/uio/uio*; do [ -e "$u" ] && echo "$(basename "$u"): $(cat "$u/name")"; done
true
