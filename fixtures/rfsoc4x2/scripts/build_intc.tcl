# rfsoc4x2_intc.xsa: AXI Timer, AXI IIC and AXI GPIO whose interrupts are combined by an AXI INTC, whose output drives
# pl_ps_irq0[0]. Nothing leaves the chip: the I2C lines have an internal pull-up (see rfsoc4x2_intc_top.v), and the
# second GPIO's output feeds the first's input, so a program raises the GPIO interrupt itself.
#   axi_gpio_0  channel 1: 4 inputs, interrupt     axi_gpio_1  channel 1: 4 outputs
# Usage: vivado -mode batch -source build_intc.tcl -tclargs <workdir> <out.xsa>
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out
set ps [hrx_project intcprj $work {PSU__USE__M_AXI_GP0 1 PSU__USE__IRQ0 1}]
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_timer axi_timer_0
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_iic axi_iic_0
set g0 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_0]
set_property -dict [list CONFIG.C_GPIO_WIDTH {4} CONFIG.C_ALL_INPUTS {1} CONFIG.C_INTERRUPT_PRESENT {1}] $g0
set g1 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_1]
set_property -dict [list CONFIG.C_GPIO_WIDTH {4} CONFIG.C_ALL_OUTPUTS {1}] $g1
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_intc axi_intc_0
hrx_concat xlconcat_0 {axi_timer_0/interrupt axi_iic_0/iic2intc_irpt axi_gpio_0/ip2intc_irpt} axi_intc_0/intr
connect_bd_net [get_bd_pins axi_intc_0/irq] [get_bd_pins zynq_ultra_ps_e_0/pl_ps_irq0]
connect_bd_net [get_bd_pins axi_gpio_1/gpio_io_o] [get_bd_pins axi_gpio_0/gpio_io_i]
foreach {ip pin} {axi_timer_0 S_AXI axi_iic_0 S_AXI axi_gpio_0 S_AXI axi_gpio_1 S_AXI axi_intc_0 s_axi} {
  hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /$ip/$pin
}
make_bd_intf_pins_external [get_bd_intf_pins axi_iic_0/IIC]
hrx_finish intcprj $work $out [file join $hrx_dir rfsoc4x2_intc_top.v]
