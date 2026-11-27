#!/usr/bin/env python3
"""Programs an RFSoC 4x2 over JTAG with a fixture design and probes it with raw register accesses (probe_<design>.tcl),
without any Hardroxide code. Volatile: nothing is written to storage.

Usage: probe.py <design>...        e.g. probe.py gpio dma          (the XSA is ../rfsoc4x2_<design>.xsa; "base"
is the base overlay, ../../board/rfsoc4x2_base_overlay.xsa)
Environment: VITIS_SETTINGS (default ~/AMD/2025.2/Vitis/settings64.sh), HRX_WORK (default target/rfsoc4x2),
HRX_RF_LOOPBACK=1 to require the tones of the RF loopback (SMA cables from the DACs to the ADCs fitted).
HRX_RF_KEEP_CLOCKS=1 to probe a board that is already running with the design loaded (`hardroxide run --xsa`), without
resetting it, and to leave its clock chips as they are: it tests whatever programmed them (rf_loopback only).
The board must be in JTAG boot mode, connected by USB, and the only JTAG board attached.
"""
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

here = Path(__file__).resolve().parent
root = here.parents[2]
work = Path(os.environ.get("HRX_WORK", root / "target" / "rfsoc4x2")) / "probe"
settings = os.environ.get("VITIS_SETTINGS", os.path.expanduser("~/AMD/2025.2/Vitis/settings64.sh"))


def addresses(hwh: bytes) -> dict[str, tuple[int, int]]:
    """Base address and size of every PL instance the processors' address map reaches."""
    out = {}
    for module in ET.fromstring(hwh).iter("MODULE"):
        if module.get("MODTYPE") == "zynq_ultra_ps_e":
            for r in module.iter("MEMRANGE"):
                if r.get("INSTANCE") and r.get("MASTERBUSINTERFACE", "").startswith("M_AXI"):
                    low = int(r.get("BASEVALUE"), 16)
                    out[r.get("INSTANCE")] = (low, int(r.get("HIGHVALUE"), 16) - low + 1)
    return out


def probe(design: str) -> bool:
    # "base" is the vendor's overlay (fixtures/board), whose handoff file keeps its own name.
    xsa = here.parent.parent / "board" / "rfsoc4x2_base_overlay.xsa" if design == "base" else here.parent / f"rfsoc4x2_{design}.xsa"
    hwh = "base.hwh" if design == "base" else "design_1.hwh"
    script = here / f"probe_{design}.tcl"
    if not xsa.exists() or not script.exists():
        sys.exit(f"missing {xsa} or {script}")
    keep = bool(os.environ.get("HRX_RF_KEEP_CLOCKS"))
    if keep and design != "rf_loopback":
        sys.exit("HRX_RF_KEEP_CLOCKS applies to rf_loopback only")
    d = work / design
    d.mkdir(parents=True, exist_ok=True)
    for stale in d.glob("*.hex"):
        stale.unlink()
    with zipfile.ZipFile(xsa) as z:
        bits = [n for n in z.namelist() if n.endswith(".bit")]
        z.extract(bits[0], d)
        z.extract("psu_init.tcl", d)
        amap = addresses(z.read(hwh))
    run = d / "run.tcl"
    lines = [f"source {{{here / 'jtag.tcl'}}}"]
    lines += [f"set base({n}) 0x{a:X}\nset size({n}) 0x{s:X}" for n, (a, s) in sorted(amap.items())]
    lines += [f"set work {{{d}}}", f"set scripts {{{here}}}",
              "if {[catch {",
              ("  connect\n  targets -set -nocase -filter {name =~ \"PSU\"}\n  set ::hrx_keep_clocks 1" if keep
               else f"  hrx_bringup {{{d / bits[0]}}} {{{d / 'psu_init.tcl'}}}"),
              f"  source {{{script}}}",
              "} message]} { puts \"PROBE FAIL  stopped: $message\"; exit 1 }",
              "puts \"PROBE done, $::hrx_failures failure(s)\"",
              "exit [expr {$::hrx_failures != 0}]"]
    run.write_text("\n".join(lines) + "\n")
    print(f"== {design}")
    r = subprocess.run(["bash", "-c", f"set +u; source '{settings}'; timeout 600 xsdb '{run}'"], capture_output=True, text=True, cwd=d)
    (d / "xsdb.log").write_text(r.stdout + r.stderr)
    for line in (r.stdout + r.stderr).splitlines():
        if line.startswith(("PROBE", "  ")) or "Timeout Reached" in line or "rror" in line:
            print(line)
    ok = r.returncode == 0 and "PROBE done, 0 failure(s)" in r.stdout and "Timeout Reached" not in r.stdout
    if design == "rf_loopback" and list(d.glob("*.hex")):
        flags = ["--require-tones"] if os.environ.get("HRX_RF_LOOPBACK") else []
        ok = subprocess.run([sys.executable, here / "rf_analyse.py", d, *flags]).returncode == 0 and ok
    return ok


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    results = {d: probe(d) for d in sys.argv[1:]}
    print("\n" + ", ".join(f"{d}: {'passed' if ok else 'FAILED'}" for d, ok in results.items()))
    sys.exit(0 if all(results.values()) else 1)
