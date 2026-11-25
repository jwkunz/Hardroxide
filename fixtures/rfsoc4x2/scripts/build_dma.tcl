# rfsoc4x2_dma.xsa: two AXI DMAs (simple mode), each with its MM2S stream looped back to S2MM through a stream FIFO.
#   axi_dma_0  32-bit addresses (low DDR only), 32-bit data, HP0_FPD, interrupts on pl_ps_irq0[1:0]
#   axi_dma_1  40-bit addresses (all of DDR),  128-bit data, HP1_FPD, interrupts on pl_ps_irq1[1:0]
# Usage: vivado -mode batch -source build_dma.tcl -tclargs <workdir> <out.xsa>
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out
set ps [hrx_project dmaprj $work {PSU__USE__M_AXI_GP0 1 PSU__USE__S_AXI_GP2 1 PSU__USE__S_AXI_GP3 1
  PSU__USE__IRQ0 1 PSU__USE__IRQ1 1}]
set d0 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_dma axi_dma_0]
set_property -dict [list CONFIG.c_include_sg {0} CONFIG.c_sg_length_width {26} CONFIG.c_addr_width {32}] $d0
set d1 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_dma axi_dma_1]
set_property -dict [list CONFIG.c_include_sg {0} CONFIG.c_sg_length_width {26} CONFIG.c_addr_width {40} \
  CONFIG.c_m_axi_mm2s_data_width {128} CONFIG.c_m_axis_mm2s_tdata_width {128} CONFIG.c_mm2s_burst_size {16} \
  CONFIG.c_m_axi_s2mm_data_width {128} CONFIG.c_s_axis_s2mm_tdata_width {128} CONFIG.c_s2mm_burst_size {16}] $d1
foreach n {0 1} {
  create_bd_cell -type ip -vlnv xilinx.com:ip:axis_data_fifo axis_data_fifo_$n
  connect_bd_intf_net [get_bd_intf_pins axi_dma_$n/M_AXIS_MM2S] [get_bd_intf_pins axis_data_fifo_$n/S_AXIS]
  connect_bd_intf_net [get_bd_intf_pins axis_data_fifo_$n/M_AXIS] [get_bd_intf_pins axi_dma_$n/S_AXIS_S2MM]
}
foreach n {0 1} { hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /axi_dma_$n/S_AXI_LITE }
foreach {n hp clk} {0 S_AXI_HP0_FPD saxihp0_fpd_aclk 1 S_AXI_HP1_FPD saxihp1_fpd_aclk} {
  hrx_axi_masters axi_smc_hp$n [list axi_dma_$n/M_AXI_MM2S axi_dma_$n/m_axi_mm2s_aclk \
    axi_dma_$n/M_AXI_S2MM axi_dma_$n/m_axi_s2mm_aclk] zynq_ultra_ps_e_0/$hp zynq_ultra_ps_e_0/$clk
}
foreach n {0 1} {
  hrx_clock axis_data_fifo_$n/s_axis_aclk
  set rst [lindex [get_bd_cells -filter {VLNV =~ *proc_sys_reset*}] 0]
  connect_bd_net [get_bd_pins $rst/peripheral_aresetn] [get_bd_pins axis_data_fifo_$n/s_axis_aresetn]
  hrx_concat xlconcat_$n [list axi_dma_$n/mm2s_introut axi_dma_$n/s2mm_introut] zynq_ultra_ps_e_0/pl_ps_irq$n
}
hrx_finish dmaprj $work $out
