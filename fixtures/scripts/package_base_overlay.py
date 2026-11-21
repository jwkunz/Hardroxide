#!/usr/bin/env python3
"""Make fixtures/board/pynq_base_overlay.xsa: the PYNQ base overlay with a ps7_init.

The base overlay XSA in fixtures/real/ is PYNQ's `.bit` and `.hwh` repackaged; it has no ps7_init,
so `hardroxide run` cannot bring a board up from it. This adds the ps7_init.{tcl,c,h} that Vivado
generates for the PS-only design built by build_pynq_base_ps7.tcl (the PYNQ-Z2 board configuration
plus the base overlay's PS-to-PL settings).

Usage: package_base_overlay.py <ps7-only.xsa> [out.xsa]
"""
import sys, zipfile, os

here = os.path.dirname(os.path.abspath(__file__))
base = os.path.join(here, "../real/pynq_base_overlay.xsa")
ps7_xsa = sys.argv[1]
out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(here, "../board/pynq_base_overlay.xsa")

with zipfile.ZipFile(base) as b, zipfile.ZipFile(ps7_xsa) as p, zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as o:
    for name in b.namelist():
        o.writestr(zipfile.ZipInfo(name, (2026, 11, 21, 0, 0, 0)), b.read(name), zipfile.ZIP_DEFLATED)
    for name in ("ps7_init.tcl", "ps7_init.c", "ps7_init.h"):
        o.writestr(zipfile.ZipInfo(name, (2026, 11, 21, 0, 0, 0)), p.read(name), zipfile.ZIP_DEFLATED)
print("wrote", out)
