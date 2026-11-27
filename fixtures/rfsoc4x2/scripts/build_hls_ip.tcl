# Packages the `vadd` and `scale` kernels (fixtures/hls, fixtures/hls_scale) as IP for the RFSoC 4x2's part.
# Usage: vitis-run --mode hls --tcl build_hls_ip.tcl --work_dir <hlswork>   (from a scratch directory; the IP lands in
# ./vaddprj/sol/impl/ip and ./scaleprj/sol/impl/ip)
set fixtures [file normalize [file dirname [info script]]/../..]
foreach {top src} {vadd hls/vadd.cpp scale hls_scale/scale.cpp} {
  open_project -reset ${top}prj
  set_top $top
  add_files $fixtures/$src
  open_solution -flow_target vivado -reset sol
  set_part xczu48dr-ffvg1517-2-e
  create_clock -period 10
  csynth_design
  export_design -format ip_catalog
  close_project
}
exit
