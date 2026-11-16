# Third-party designs

`pynq_base_overlay.xsa` and `pynq_logictools_overlay.xsa` are the `base` and
`logictools` overlays shipped with PYNQ (https://github.com/Xilinx/PYNQ,
Apache-2.0), repackaged as XSA archives from their `.bit` and `.hwh` files
(read from a PYNQ 3.1 image). They are not built by Hardroxide's scripts, which
makes them a check that the tool copes with real designs: dozens of IP
instances, MicroBlaze subsystems with private address maps, hierarchy, and
custom IP.

`hwh/` holds two handoff files from other Vivado versions, taken from the
`pynqmetadata` test data that ships with PYNQ (Apache-2.0): an HLS resizer on a
Zynq-7000 from Vivado 2019.2, and an RFSoC design (UltraScale+) from Vivado
2020.2. They check that the parser copes with older tool versions and that the
generator refuses architectures it does not support. They have no bitstream.
