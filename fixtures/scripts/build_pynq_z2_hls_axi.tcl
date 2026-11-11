# Builds fixtures/pynq_z2_hls_scale.xsa: PS7 (FCLK0=100MHz, HP0 enabled) + the HLS
# `scale` kernel with two m_axi masters (gmem0, gmem1) into S_AXI_HP0; its
# interrupt goes to IRQ_F2P[0]. Package the IP first (fixtures/hls_scale/build_ip.tcl).
# Usage: vivado -mode batch -source build_pynq_z2_hls_axi.tcl -tclargs <workdir> <ip_repo> <out.xsa>
set work [lindex $argv 0]
set iprepo [lindex $argv 1]
set out  [lindex $argv 2]
create_project -force scaleprj $work -part xc7z020clg400-1
set_property ip_repo_paths $iprepo [current_project]
update_ip_catalog
create_bd_design design_1
set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7 processing_system7_0]
set_property -dict [list \
  CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ {100} \
  CONFIG.PCW_CRYSTAL_PERIPHERAL_FREQMHZ {50} \
  CONFIG.PCW_USE_M_AXI_GP0 {1} CONFIG.PCW_USE_S_AXI_HP0 {1} \
  CONFIG.PCW_USE_FABRIC_INTERRUPT {1} CONFIG.PCW_IRQ_F2P_INTR {1} \
  CONFIG.PCW_UIPARAM_DDR_PARTNO {MT41K256M16 RE-125} \
  CONFIG.PCW_UIPARAM_DDR_BUS_WIDTH {16 Bit} \
  CONFIG.PCW_EN_CLK0_PORT {1} CONFIG.PCW_EN_RST0_PORT {1}] $ps
create_bd_cell -type ip -vlnv xilinx.com:hls:scale scale_0
connect_bd_net [get_bd_pins scale_0/interrupt] [get_bd_pins processing_system7_0/IRQ_F2P]
apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
  Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/processing_system7_0/M_AXI_GP0} \
  Slave {/scale_0/s_axi_control} intc_ip {New AXI Interconnect} master_apm {0}} [get_bd_intf_pins scale_0/s_axi_control]
apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
  Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/scale_0/m_axi_gmem0} \
  Slave {/processing_system7_0/S_AXI_HP0} intc_ip {New AXI Interconnect} master_apm {0}} [get_bd_intf_pins processing_system7_0/S_AXI_HP0]
apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
  Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/scale_0/m_axi_gmem1} \
  Slave {/processing_system7_0/S_AXI_HP0} intc_ip {Auto} master_apm {0}} [get_bd_intf_pins scale_0/m_axi_gmem1]
assign_bd_address
validate_bd_design
save_bd_design
make_wrapper -files [get_files design_1.bd] -top
add_files -norecurse [glob $work/scaleprj.gen/sources_1/bd/design_1/hdl/design_1_wrapper.v]
set_property top design_1_wrapper [current_fileset]
launch_runs impl_1 -to_step write_bitstream -jobs 8
wait_on_run impl_1
write_hw_platform -fixed -include_bit -force $out
