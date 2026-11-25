# rfsoc4x2_gpio.xsa: the board's LEDs, buttons and switches on AXI GPIO, through HPM0_FPD.
#   axi_gpio_0  channel 1: the four white LEDs (out); channel 2: buttons 0..4 (bits 0..4) and switches 0..3
#               (bits 5..8), with the interrupt on pl_ps_irq0[0]
#   axi_gpio_1  channel 1: RGB LED 0 (bits 0..2 = R, G, B) and RGB LED 1 (bits 3..5)
# Also serves as the processor-only design: its psu_init is the board's.
# Usage: vivado -mode batch -source build_gpio.tcl -tclargs <workdir> <out.xsa>
# Bit 4 (pin AN12) is the board's user reset button (URST_B), active low: it reads 1 until pressed.
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out
set ps [hrx_project gpio $work {PSU__USE__M_AXI_GP0 1 PSU__USE__IRQ0 1}]
set g0 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_0]
set_property -dict [list CONFIG.C_GPIO_WIDTH {4} CONFIG.C_ALL_OUTPUTS {1} CONFIG.C_IS_DUAL {1} \
  CONFIG.C_GPIO2_WIDTH {9} CONFIG.C_ALL_INPUTS_2 {1} CONFIG.C_INTERRUPT_PRESENT {1}] $g0
set g1 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_1]
set_property -dict [list CONFIG.C_GPIO_WIDTH {6} CONFIG.C_ALL_OUTPUTS {1}] $g1
foreach ip {axi_gpio_0 axi_gpio_1} { hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /$ip/S_AXI }
connect_bd_net [get_bd_pins axi_gpio_0/ip2intc_irpt] [get_bd_pins zynq_ultra_ps_e_0/pl_ps_irq0]
create_bd_port -dir O -from 3 -to 0 leds
create_bd_port -dir I -from 8 -to 0 btn_sw
create_bd_port -dir O -from 5 -to 0 rgb
connect_bd_net [get_bd_pins axi_gpio_0/gpio_io_o] [get_bd_ports leds]
connect_bd_net [get_bd_ports btn_sw] [get_bd_pins axi_gpio_0/gpio2_io_i]
connect_bd_net [get_bd_pins axi_gpio_1/gpio_io_o] [get_bd_ports rgb]
# Pins: Real Digital's 4x2_LED_PB__SW.xdc.
set xdc $work/pins.xdc
file mkdir $work
set f [open $xdc w]
foreach {port pins} {
  leds   {AR11 AW10 AT11 AU10}
  btn_sw {AV12 AV10 AW9 AT12 AN12 AN13 AU12 AW11 AV11}
  rgb    {AM8 AM7 AN8 AR12 AP8 AT10}
} {
  set i 0
  foreach p $pins { puts $f "set_property -dict {PACKAGE_PIN $p IOSTANDARD LVCMOS18} \[get_ports {$port\[$i\]}\]"; incr i }
}
puts $f {set_false_path -to [get_ports {leds[*] rgb[*]}]}
puts $f {set_false_path -from [get_ports {btn_sw[*]}]}
close $f
file copy -force $xdc $work.pins.xdc
hrx_finish gpio $work $out "" [list $work.pins.xdc]
