#!/usr/bin/env python3
"""Make fixtures/board/rfsoc4x2_base_overlay.xsa: the RFSoC 4x2 base overlay with a psu_init.

The bitstream and hardware handoff are RFSoC-PYNQ's own, read (without root, and without writing) from the root file
system of its SD card image. The image has no psu_init for them, so `hardroxide run` could not bring a board up from
them alone: this adds the one Vivado generates for the processor-only design of
fixtures/rfsoc4x2/scripts/build_base_ps.tcl (the board preset plus the overlay's PS-to-PL settings).

Usage: package_rfsoc4x2_base.py <RFSoC4x2-x.y.z.img> <ps-only.xsa> [out.xsa]     (needs sfdisk and debugfs)
"""
import json, os, subprocess, sys, tempfile, zipfile

here = os.path.dirname(os.path.abspath(__file__))
if len(sys.argv) < 3:
    sys.exit(__doc__)
image, ps_xsa = os.path.abspath(sys.argv[1]), sys.argv[2]
out = sys.argv[3] if len(sys.argv) > 3 else os.path.join(here, "../board/rfsoc4x2_base_overlay.xsa")
if "?" in image:
    sys.exit("the image's path must not contain '?'")

SBIN = os.environ.get("PATH", "") + ":/usr/sbin:/sbin"
env = dict(os.environ, PATH=SBIN)
table = json.loads(subprocess.run(["sfdisk", "-J", image], capture_output=True, check=True, env=env).stdout)["partitiontable"]
linux = [p for p in table["partitions"] if p["type"] == "83"]
if len(linux) != 1:
    sys.exit(f"expected one Linux partition, found {len(linux)}")
volume = f"{image}?offset={linux[0]['start'] * table.get('sectorsize', 512)}"
overlay = "/usr/local/share/pynq-venv/lib/python3.12/site-packages/pynq/overlays/base"

with tempfile.TemporaryDirectory() as tmp:
    files = {}
    for name in ("base.bit", "base.hwh"):
        path = os.path.join(tmp, name)
        # debugfs opens the file system read-only unless given -w; it reports a missing file but exits with success.
        subprocess.run(["debugfs", "-R", f"dump {overlay}/{name} {path}", volume], capture_output=True, check=True, env=env)
        if not os.path.exists(path) or os.path.getsize(path) == 0:
            sys.exit(f"{overlay}/{name} is not in the image")
        with open(path, "rb") as f:
            files[name] = f.read()

with zipfile.ZipFile(ps_xsa) as p:
    for name in ("psu_init.tcl", "psu_init.c", "psu_init.h"):
        files[name] = p.read(name)
    # The export's index files, which the vendor's tools need to build a boot loader (`hardroxide package`).
    for name in ("xsa.xml", "hwdef.xml"):
        files[name] = p.read(name).replace(b'Name="design_1.hwh"', b'Name="base.hwh"')

with zipfile.ZipFile(out, "w") as o:
    for name, data in files.items():
        # A fixed time, so that the same inputs give the same archive.
        info = zipfile.ZipInfo(name, (2026, 11, 25, 0, 0, 0))
        o.writestr(info, data, zipfile.ZIP_DEFLATED, 9)
print("wrote", out)
