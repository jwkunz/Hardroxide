# Builds a PS-only XSA with the PYNQ-Z2 board's PS configuration and the PS-to-PL settings the PYNQ base
# overlay uses (clocks FCLK0..3 = 100 / 142.857 / 200 / 100 MHz, both master GPs, a slave GP, HP0 and HP2,
# fabric interrupts, EMIO GPIO and I2C 1). Its only purpose is the `ps7_init.tcl` that Vivado generates
# from this configuration: fixtures/scripts/package_base_overlay.py adds it to the base overlay's own
# bitstream and handoff, because the base overlay XSA that PYNQ ships carries no ps7_init (and its DDR
# settings, applied through Vivado, failed a memory test on the board; the stock-part DDR settings below pass).
# Usage: vivado -mode batch -source build_pynq_base_ps7.tcl -tclargs <workdir> <out.xsa>
set work [lindex $argv 0]
set out  [lindex $argv 1]
create_project -force baseps7 $work -part xc7z020clg400-1
create_bd_design design_1
set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7 processing_system7_0]
set_property -dict [list \
  CONFIG.PCW_CRYSTAL_PERIPHERAL_FREQMHZ {50} \
  CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ {100} \
  CONFIG.PCW_FPGA1_PERIPHERAL_FREQMHZ {142.857} \
  CONFIG.PCW_FPGA2_PERIPHERAL_FREQMHZ {200} \
  CONFIG.PCW_FPGA3_PERIPHERAL_FREQMHZ {100} \
  CONFIG.PCW_EN_CLK0_PORT {1} CONFIG.PCW_EN_CLK1_PORT {1} CONFIG.PCW_EN_CLK2_PORT {1} CONFIG.PCW_EN_CLK3_PORT {1} \
  CONFIG.PCW_EN_RST0_PORT {1} \
  CONFIG.PCW_USE_M_AXI_GP0 {1} CONFIG.PCW_USE_M_AXI_GP1 {1} CONFIG.PCW_USE_S_AXI_GP0 {1} \
  CONFIG.PCW_USE_S_AXI_HP0 {1} CONFIG.PCW_USE_S_AXI_HP2 {1} \
  CONFIG.PCW_USE_FABRIC_INTERRUPT {1} CONFIG.PCW_IRQ_F2P_INTR {1} \
  CONFIG.PCW_GPIO_EMIO_GPIO_ENABLE {1} CONFIG.PCW_EN_EMIO_I2C1 {1} \
  CONFIG.PCW_UIPARAM_DDR_PARTNO {MT41K256M16 RE-125} \
  CONFIG.PCW_UIPARAM_DDR_BUS_WIDTH {16 Bit}] $ps
source [file join [file dirname [info script]] board/pynq_z2_ps7_board.tcl]
apply_pynq_z2_ps7 $ps
# Nothing is attached to the PL ports; the AXI port clocks only need a source for the design to generate.
foreach pin {M_AXI_GP0_ACLK M_AXI_GP1_ACLK S_AXI_GP0_ACLK S_AXI_HP0_ACLK S_AXI_HP2_ACLK} {
  connect_bd_net [get_bd_pins processing_system7_0/FCLK_CLK0] [get_bd_pins processing_system7_0/$pin]
}
save_bd_design
generate_target all [get_files design_1.bd]
write_hw_platform -fixed -force $out
