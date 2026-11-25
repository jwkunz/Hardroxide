# rfsoc4x2_hls_vadd.xsa: the HLS `vadd` kernel (AXI-Lite only) with its interrupt on pl_ps_irq0[0].
# Package the IP first (build_hls_ip.tcl).
# Usage: vivado -mode batch -source build_hls_vadd.tcl -tclargs <workdir> <out.xsa> <ip_repo>
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out iprepo
set ps [hrx_project hlsprj $work {PSU__USE__M_AXI_GP0 1 PSU__USE__IRQ0 1} $iprepo]
create_bd_cell -type ip -vlnv xilinx.com:hls:vadd vadd_0
hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /vadd_0/s_axi_control
connect_bd_net [get_bd_pins vadd_0/interrupt] [get_bd_pins zynq_ultra_ps_e_0/pl_ps_irq0]
hrx_finish hlsprj $work $out
