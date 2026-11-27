# rfsoc4x2_loopback.xsa: peripherals wired back to themselves inside the FPGA, so on-board tests need no pins and no
# cables (the Zynq-7000 fixtures `peripherals`, `loopback`, `mem_uart` and `fifo` in one design).
#   axi_uartlite_0 <-> axi_uartlite_1     tx to rx, both ways
#   axi_iic_0, axi_iic_1                  share one open-drain SDA and SCL (wired-AND)
#   axi_quad_spi_0                        standard mode, MOSI to MISO
#   axi_gpio_1 -> axi_gpio_0              8 bits; axi_gpio_0 raises an interrupt
#   axi_fifo_mm_s_0                       transmit stream to receive stream
#   axi_bram_ctrl_0 + block RAM           64 KiB
#   axi_timer_0
# Control is through HPM0_LPD (the low-power domain's master), unlike the other designs.
# pl_ps_irq0[0..7]: timer, uart0, uart1, iic0, iic1, spi, gpio0, fifo
# Usage: vivado -mode batch -source build_loopback.tcl -tclargs <workdir> <out.xsa>
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out
set ps [hrx_project lbprj $work {PSU__USE__M_AXI_GP2 1 PSU__USE__IRQ0 1}]
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_timer axi_timer_0
foreach n {0 1} {
  set u [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_uartlite axi_uartlite_$n]
  set_property CONFIG.C_BAUDRATE {115200} $u
  create_bd_cell -type ip -vlnv xilinx.com:ip:axi_iic axi_iic_$n
}
set spi [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_quad_spi axi_quad_spi_0]
set_property -dict [list CONFIG.C_SPI_MODE {0} CONFIG.C_NUM_SS_BITS {1}] $spi
set g0 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_0]
set_property -dict [list CONFIG.C_GPIO_WIDTH {8} CONFIG.C_ALL_INPUTS {1} CONFIG.C_INTERRUPT_PRESENT {1}] $g0
set g1 [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_1]
set_property -dict [list CONFIG.C_GPIO_WIDTH {8} CONFIG.C_ALL_OUTPUTS {1}] $g1
set fifo [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_fifo_mm_s axi_fifo_mm_s_0]
set_property -dict [list CONFIG.C_USE_TX_CTRL {0}] $fifo
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_bram_ctrl axi_bram_ctrl_0
set_property CONFIG.SINGLE_PORT_BRAM {1} [get_bd_cells axi_bram_ctrl_0]
create_bd_cell -type ip -vlnv xilinx.com:ip:blk_mem_gen blk_mem_gen_0
connect_bd_intf_net [get_bd_intf_pins axi_bram_ctrl_0/BRAM_PORTA] [get_bd_intf_pins blk_mem_gen_0/BRAM_PORTA]
hrx_concat xlconcat_0 {axi_timer_0/interrupt axi_uartlite_0/interrupt axi_uartlite_1/interrupt axi_iic_0/iic2intc_irpt
  axi_iic_1/iic2intc_irpt axi_quad_spi_0/ip2intc_irpt axi_gpio_0/ip2intc_irpt axi_fifo_mm_s_0/interrupt} zynq_ultra_ps_e_0/pl_ps_irq0
foreach {ip pin} {axi_timer_0 S_AXI axi_uartlite_0 S_AXI axi_uartlite_1 S_AXI axi_iic_0 S_AXI axi_iic_1 S_AXI
  axi_quad_spi_0 AXI_LITE axi_gpio_0 S_AXI axi_gpio_1 S_AXI axi_fifo_mm_s_0 S_AXI axi_bram_ctrl_0 S_AXI} {
  hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_LPD /$ip/$pin
}
set_property range 64K [get_bd_addr_segs -of_objects [get_bd_addr_spaces zynq_ultra_ps_e_0/Data] -filter {NAME =~ *bram*}]
connect_bd_net [get_bd_pins zynq_ultra_ps_e_0/pl_clk0] [get_bd_pins axi_quad_spi_0/ext_spi_clk]
connect_bd_net [get_bd_pins axi_gpio_1/gpio_io_o] [get_bd_pins axi_gpio_0/gpio_io_i]
connect_bd_intf_net [get_bd_intf_pins axi_fifo_mm_s_0/AXI_STR_TXD] [get_bd_intf_pins axi_fifo_mm_s_0/AXI_STR_RXD]
foreach n {0 1} {
  make_bd_intf_pins_external [get_bd_intf_pins axi_uartlite_$n/UART]
  make_bd_intf_pins_external [get_bd_intf_pins axi_iic_$n/IIC]
}
make_bd_intf_pins_external [get_bd_intf_pins axi_quad_spi_0/SPI_0]
hrx_finish lbprj $work $out [file join $hrx_dir rfsoc4x2_loopback_top.v]
