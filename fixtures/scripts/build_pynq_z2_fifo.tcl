# Builds fixtures/pynq_z2_fifo.xsa: PS7 + AXI4-Stream FIFO (axi_fifo_mm_s, AXI4-Lite
# register interface) with its stream transmit port looped back into its receive
# port, so software can push a packet and read it back. Interrupt on IRQ_F2P[0].
# Usage: vivado -mode batch -source build_pynq_z2_fifo.tcl -tclargs <workdir> <out.xsa>
set work [lindex $argv 0]
set out  [lindex $argv 1]
create_project -force fifoprj $work -part xc7z020clg400-1
create_bd_design design_1
set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7 processing_system7_0]
set_property -dict [list \
  CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ {100} \
  CONFIG.PCW_CRYSTAL_PERIPHERAL_FREQMHZ {50} \
  CONFIG.PCW_USE_M_AXI_GP0 {1} \
  CONFIG.PCW_USE_FABRIC_INTERRUPT {1} CONFIG.PCW_IRQ_F2P_INTR {1} \
  CONFIG.PCW_UIPARAM_DDR_PARTNO {MT41K256M16 RE-125} \
  CONFIG.PCW_UIPARAM_DDR_BUS_WIDTH {16 Bit} \
  CONFIG.PCW_EN_CLK0_PORT {1} CONFIG.PCW_EN_RST0_PORT {1}] $ps
set fifo [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_fifo_mm_s axi_fifo_mm_s_0]
connect_bd_intf_net [get_bd_intf_pins axi_fifo_mm_s_0/AXI_STR_TXD] [get_bd_intf_pins axi_fifo_mm_s_0/AXI_STR_RXD]
connect_bd_net [get_bd_pins axi_fifo_mm_s_0/interrupt] [get_bd_pins processing_system7_0/IRQ_F2P]
apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
  Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/processing_system7_0/M_AXI_GP0} \
  Slave {/axi_fifo_mm_s_0/S_AXI} intc_ip {New AXI Interconnect} master_apm {0}} [get_bd_intf_pins axi_fifo_mm_s_0/S_AXI]
assign_bd_address
validate_bd_design
save_bd_design
make_wrapper -files [get_files design_1.bd] -top
add_files -norecurse [glob $work/fifoprj.gen/sources_1/bd/design_1/hdl/design_1_wrapper.v]
set_property top design_1_wrapper [current_fileset]
launch_runs impl_1 -to_step write_bitstream -jobs 8
wait_on_run impl_1
write_hw_platform -fixed -include_bit -force $out
