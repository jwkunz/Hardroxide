# Builds fixtures/pynq_z2_peripherals.xsa: PS7 + AXI Timer + AXI IIC + AXI Quad
# SPI (standard mode). Interrupts via xlconcat to IRQ_F2P[2:0]. The IIC/SPI
# interfaces are made external but NOT pin-constrained (NSTD-1/UCIO-1 DRCs are
# downgraded to warnings in a write_bitstream pre-hook), so this fixture only exercises the interface model.
# Usage: vivado -mode batch -source build_pynq_z2_peripherals.tcl -tclargs <workdir> <out.xsa>
set work [lindex $argv 0]
set out  [lindex $argv 1]
create_project -force perprj $work -part xc7z020clg400-1
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
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_timer axi_timer_0
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_iic axi_iic_0
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_quad_spi axi_quad_spi_0
set_property -dict [list CONFIG.C_SPI_MODE {0} CONFIG.C_NUM_SS_BITS {1}] [get_bd_cells axi_quad_spi_0]
set cc [create_bd_cell -type ip -vlnv xilinx.com:ip:xlconcat xlconcat_0]
set_property CONFIG.NUM_PORTS {3} $cc
connect_bd_net [get_bd_pins axi_timer_0/interrupt] [get_bd_pins xlconcat_0/In0]
connect_bd_net [get_bd_pins axi_iic_0/iic2intc_irpt] [get_bd_pins xlconcat_0/In1]
connect_bd_net [get_bd_pins axi_quad_spi_0/ip2intc_irpt] [get_bd_pins xlconcat_0/In2]
connect_bd_net [get_bd_pins xlconcat_0/dout] [get_bd_pins processing_system7_0/IRQ_F2P]
foreach {ip pin} {axi_timer_0 S_AXI axi_iic_0 S_AXI axi_quad_spi_0 AXI_LITE} {
  apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
    Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/processing_system7_0/M_AXI_GP0} \
    Slave "/$ip/$pin" intc_ip {New AXI Interconnect} master_apm {0}} [get_bd_intf_pins $ip/$pin]
}
make_bd_intf_pins_external [get_bd_intf_pins axi_iic_0/IIC]
make_bd_intf_pins_external [get_bd_intf_pins axi_quad_spi_0/SPI_0]
connect_bd_net [get_bd_pins processing_system7_0/FCLK_CLK0] [get_bd_pins axi_quad_spi_0/ext_spi_clk]
assign_bd_address
validate_bd_design
save_bd_design
make_wrapper -files [get_files design_1.bd] -top
add_files -norecurse [glob $work/perprj.gen/sources_1/bd/design_1/hdl/design_1_wrapper.v]
set_property top design_1_wrapper [current_fileset]
# DRC downgrade must run inside the implementation run (pre-write_bitstream hook).
set hook $work/drc_warn.tcl
set f [open $hook w]
puts $f "set_property SEVERITY {Warning} \[get_drc_checks {NSTD-1 UCIO-1}\]"
close $f
add_files -fileset utils_1 -norecurse $hook
set_property STEPS.WRITE_BITSTREAM.TCL.PRE $hook [get_runs impl_1]
launch_runs impl_1 -to_step write_bitstream -jobs 8
wait_on_run impl_1
write_hw_platform -fixed -include_bit -force $out
