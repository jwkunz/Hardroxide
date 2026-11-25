# rfsoc4x2_hls_scale.xsa: the HLS `scale` kernel with two memory-mapped masters (64-bit address registers) into
# HP0_FPD; its interrupt is on pl_ps_irq0[0]. Package the IP first (build_hls_ip.tcl).
# Usage: vivado -mode batch -source build_hls_scale.tcl -tclargs <workdir> <out.xsa> <ip_repo>
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out iprepo
set ps [hrx_project scaleprj $work {PSU__USE__M_AXI_GP0 1 PSU__USE__S_AXI_GP2 1 PSU__USE__IRQ0 1} $iprepo]
create_bd_cell -type ip -vlnv xilinx.com:hls:scale scale_0
connect_bd_net [get_bd_pins scale_0/interrupt] [get_bd_pins zynq_ultra_ps_e_0/pl_ps_irq0]
hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /scale_0/s_axi_control
hrx_axi_masters axi_smc_hp0 {scale_0/m_axi_gmem0 scale_0/ap_clk scale_0/m_axi_gmem1 scale_0/ap_clk} \
  zynq_ultra_ps_e_0/S_AXI_HP0_FPD zynq_ultra_ps_e_0/saxihp0_fpd_aclk
hrx_finish scaleprj $work $out
