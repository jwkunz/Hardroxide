#!/bin/sh
# Run ON THE BOARD, as root: free the PL so a design can be loaded safely.
#
# Reprogramming the PL while a kernel driver or service is using it can freeze
# the CPU (docs/08-verification-gaps.md, G16). This stops the services and
# unloads/unbinds the drivers that sit on the PL, then lists anything still bound.
# It changes only the running system; a reboot restores the stock desktop.
#
# Defaults suit the PYNQ-Z2 image used for validation; override with
#   HRX_SERVICES="jupyter ..."   systemd services to stop
#   HRX_MODULES="zocl ..."       kernel modules to remove (if loaded)
set -u
[ "$(id -u)" = 0 ] || { echo "run as root: sudo $0" >&2; exit 1; }
SERVICES=${HRX_SERVICES:-"panther-desktop jupyter pynq"}
MODULES=${HRX_MODULES:-"panther_drm panther_audio zocl"}

for s in $SERVICES; do
  if systemctl list-unit-files "$s.service" 2>/dev/null | grep -q "$s.service"; then
    systemctl stop "$s" 2>/dev/null && echo "stopped service $s"
  fi
done
sleep 1
for m in $MODULES; do
  if lsmod | grep -q "^$m "; then
    rmmod "$m" 2>/dev/null && echo "removed module $m" || echo "could not remove module $m (still in use?)"
  fi
done
# Platform devices at PL addresses (0x40000000..0xBFFFFFFF) with a driver bound,
# except the passive generic UIO driver.
left=0
for d in /sys/bus/platform/devices/*; do
  n=$(basename "$d"); a=${n%%.*}
  case $a in *[!0-9a-fA-F]*|"") continue ;; esac
  v=$((0x$a))
  [ "$v" -ge $((0x40000000)) ] && [ "$v" -le $((0xBFFFFFFF)) ] || continue
  [ -e "$d/driver" ] || continue
  drv=$(basename "$(readlink "$d/driver")")
  [ "$drv" = uio_pdrv_genirq ] && continue
  if echo "$n" > "$d/driver/unbind" 2>/dev/null; then echo "unbound $n ($drv)"; else echo "STILL BOUND: $n ($drv)"; left=1; fi
done
[ "$left" = 0 ] && echo "ready: no driver is bound to a PL address" || { echo "not ready: see above" >&2; exit 1; }
