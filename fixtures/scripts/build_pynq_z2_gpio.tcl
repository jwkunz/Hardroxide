# Builds fixtures/pynq_z2_gpio.xsa: Zynq PS (FCLK0=100MHz) + AXI GPIO
# (ch1: 4 LEDs out, ch2: 4 buttons + 2 switches in) for the PYNQ-Z2.
# Usage: vivado -mode batch -source build_pynq_z2_gpio.tcl -tclargs <workdir> <out.xsa>
set work [lindex $argv 0]
set out  [lindex $argv 1]
create_project -force gpio $work -part xc7z020clg400-1
create_bd_design design_1
set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7 processing_system7_0]
set_property -dict [list \
  CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ {100} \
  CONFIG.PCW_USE_M_AXI_GP0 {1} \
  CONFIG.PCW_UIPARAM_DDR_PARTNO {MT41K256M16 RE-125} \
  CONFIG.PCW_UIPARAM_DDR_BUS_WIDTH {16 Bit} \
  CONFIG.PCW_EN_CLK0_PORT {1} CONFIG.PCW_EN_RST0_PORT {1}] $ps
set gpio [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_0]
set_property -dict [list \
  CONFIG.C_GPIO_WIDTH {4} CONFIG.C_ALL_OUTPUTS {1} \
  CONFIG.C_IS_DUAL {1} CONFIG.C_GPIO2_WIDTH {6} CONFIG.C_ALL_INPUTS_2 {1}] $gpio
apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
  Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/processing_system7_0/M_AXI_GP0} \
  Slave {/axi_gpio_0/S_AXI} intc_ip {New AXI Interconnect} master_apm {0}} [get_bd_intf_pins axi_gpio_0/S_AXI]
create_bd_port -dir O -from 3 -to 0 leds
create_bd_port -dir I -from 5 -to 0 btn_sw
connect_bd_net [get_bd_pins axi_gpio_0/gpio_io_o] [get_bd_ports leds]
connect_bd_net [get_bd_ports btn_sw] [get_bd_pins axi_gpio_0/gpio2_io_i]
assign_bd_address
validate_bd_design
save_bd_design
make_wrapper -files [get_files design_1.bd] -top
add_files -norecurse [glob $work/gpio.gen/sources_1/bd/design_1/hdl/design_1_wrapper.v]
set_property top design_1_wrapper [current_fileset]
set xdc $work/pins.xdc
set f [open $xdc w]
set i 0
foreach p {R14 P14 N16 M14} { puts $f "set_property -dict {PACKAGE_PIN $p IOSTANDARD LVCMOS33} \[get_ports {leds\[$i\]}\]"; incr i }
set i 0
foreach p {D19 D20 L20 L19 M20 M19} { puts $f "set_property -dict {PACKAGE_PIN $p IOSTANDARD LVCMOS33} \[get_ports {btn_sw\[$i\]}\]"; incr i }
close $f
add_files -fileset constrs_1 $xdc
launch_runs impl_1 -to_step write_bitstream -jobs 8
wait_on_run impl_1
write_hw_platform -fixed -include_bit -force $out
