# Packages the `scale` kernel as an IP-catalog IP for Vivado.
# Usage: vitis-run --mode hls --tcl build_ip.tcl --work_dir <hlswork>
# (run from a scratch directory; the IP lands in ./scaleprj/sol/impl/ip)
open_project -reset scaleprj
set_top scale
add_files [file join [file dirname [info script]] scale.cpp]
open_solution -flow_target vivado -reset sol
set_part xc7z020clg400-1
create_clock -period 10
csynth_design
export_design -format ip_catalog
