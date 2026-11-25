# rfsoc4x2_rf_loopback.xsa: two RF DACs fed by waveform players, two RF ADCs captured to memory, for a cable from a DAC
# to an ADC (ADR 0005; the converters' driver is 3.1, the design is built while the license lasts).
#
# Converters (clocks and rates as in the PYNQ base overlay for this board, from the board's LMX2594s at 491.52 MHz,
# sampling at 4915.2 MS/s):
#   DAC tile 0 and tile 2, block 0 (rfdc vout00 = connector DAC B, vout20 = DAC A): I/Q input, fine mixer, interpolation 2; 8 I/Q pairs
#                                                         (256 bits) per 307.2 MHz clock
#   ADC tile 2, both blocks (vin2_01 = connector ADC B, vin2_23 = ADC A):           real samples, mixer bypassed (coarse), decimation 2; 8 samples
#                                                         (128 bits) per 307.2 MHz clock, 2457.6 MS/s
#   The converters work only once the board's clock chips (LMK04828, LMX2594, on the PS's SPI 0) are programmed.
#
# Control, all on HPM0_FPD at 100 MHz:
#   rfdc                          the data converter's registers
#   hrx_player_N (N = 0: DAC tile 0, 1: DAC tile 2), see ../hdl/hrx_player.v:
#     axi_bram_ctrl_N             its 128 KiB waveform memory (4096 words of 256 bits)
#     axi_gpio_play_N             channel 1: ctrl, channel 2: the constant I/Q
#   hrx_capture_0, see ../hdl/hrx_capture.v (input 0: vin2_01, input 1: vin2_23):
#     axi_gpio_capture            channel 1: ctrl, channel 2: status (in)
#     axi_dma_0                   S2MM only, 40-bit addresses, HP0_FPD; a 32768-word FIFO (512 KiB) sits before it,
#                                 so captures up to that length lose nothing
#   pl_ps_irq0[0]: DMA done, pl_ps_irq0[1]: data converter
# Usage: vivado -mode batch -source build_rf_loopback.tcl -tclargs <workdir> <out.xsa>
source [file join [file dirname [info script]] common.tcl]
lassign $argv work out
set ps [hrx_project rfprj $work {PSU__USE__M_AXI_GP0 1 PSU__USE__S_AXI_GP2 1 PSU__USE__IRQ0 1}]
add_files -norecurse [list $hrx_dir/../hdl/hrx_player.v $hrx_dir/../hdl/hrx_capture.v]
set_property XPM_LIBRARIES {XPM_CDC} [current_project]
update_compile_order -fileset sources_1

# ---- data converter -------------------------------------------------------------------------------------------------
set rf [create_bd_cell -type ip -vlnv xilinx.com:ip:usp_rf_data_converter rfdc]
set config {
  ADC_Slice00_Enable false ADC_Slice02_Enable false ADC_Slice20_Enable true ADC_Slice22_Enable true
  ADC2_PLL_Enable true ADC2_Refclk_Freq 491.520 ADC2_Sampling_Rate 4.9152 ADC2_Outclk_Freq 307.200
  ADC_Data_Type20 0 ADC_Data_Type22 0 ADC_Mixer_Type20 1 ADC_Mixer_Type22 1
  ADC_Decimation_Mode20 2 ADC_Decimation_Mode22 2 ADC_Data_Width20 8 ADC_Data_Width22 8 ADC_OBS22 false
  DAC_Slice00_Enable true DAC_Slice20_Enable true
  DAC0_PLL_Enable true DAC0_Refclk_Freq 491.520 DAC0_Sampling_Rate 4.9152 DAC0_Outclk_Freq 307.200
  DAC2_PLL_Enable true DAC2_Refclk_Freq 491.520 DAC2_Sampling_Rate 4.9152 DAC2_Outclk_Freq 307.200
  DAC_Interpolation_Mode00 2 DAC_Interpolation_Mode20 2 DAC_Mixer_Type00 2 DAC_Mixer_Type20 2
  DAC_Mixer_Mode00 0 DAC_Mixer_Mode20 0 DAC_Data_Width00 16 DAC_Data_Width20 16
}
set d {}
foreach {k v} $config { lappend d CONFIG.$k $v }
set_property -dict $d $rf
foreach p {adc2_clk dac0_clk dac2_clk sysref_in vin2_01 vin2_23 vout00 vout20} {
  make_bd_intf_pins_external -name $p [get_bd_intf_pins rfdc/$p]
}
foreach p {adc2_clk dac0_clk dac2_clk} { set_property CONFIG.FREQ_HZ 491520000 [get_bd_intf_ports $p] }
hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /rfdc/s_axi

# One reset per converter clock; they stay in reset until the converter's clock runs.
foreach clk {clk_dac0 clk_dac2 clk_adc2} {
  set r [create_bd_cell -type ip -vlnv xilinx.com:ip:proc_sys_reset rst_$clk]
  connect_bd_net [get_bd_pins rfdc/$clk] [get_bd_pins rst_$clk/slowest_sync_clk]
  connect_bd_net [get_bd_pins zynq_ultra_ps_e_0/pl_resetn0] [get_bd_pins rst_$clk/ext_reset_in]
}
foreach {axis clk} {s0 clk_dac0 s2 clk_dac2 m2 clk_adc2} {
  connect_bd_net [get_bd_pins rfdc/$clk] [get_bd_pins rfdc/${axis}_axis_aclk]
  connect_bd_net [get_bd_pins rst_$clk/peripheral_aresetn] [get_bd_pins rfdc/${axis}_axis_aresetn]
}

# ---- transmit -------------------------------------------------------------------------------------------------------
foreach {n clk dac} {0 clk_dac0 s00_axis 1 clk_dac2 s20_axis} {
  create_bd_cell -type module -reference hrx_player hrx_player_$n
  connect_bd_net [get_bd_pins rfdc/$clk] [get_bd_pins hrx_player_$n/clk]
  connect_bd_net [get_bd_pins rst_$clk/peripheral_aresetn] [get_bd_pins hrx_player_$n/aresetn]
  connect_bd_intf_net [get_bd_intf_pins hrx_player_$n/m_axis] [get_bd_intf_pins rfdc/$dac]
  set c [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_bram_ctrl axi_bram_ctrl_$n]
  set_property -dict [list CONFIG.SINGLE_PORT_BRAM {1} CONFIG.DATA_WIDTH {256}] $c
  set m [create_bd_cell -type ip -vlnv xilinx.com:ip:blk_mem_gen blk_mem_gen_$n]
  set_property CONFIG.Memory_Type {True_Dual_Port_RAM} $m
  connect_bd_intf_net [get_bd_intf_pins axi_bram_ctrl_$n/BRAM_PORTA] [get_bd_intf_pins blk_mem_gen_$n/BRAM_PORTA]
  connect_bd_intf_net [get_bd_intf_pins hrx_player_$n/bram] [get_bd_intf_pins blk_mem_gen_$n/BRAM_PORTB]
  set g [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_play_$n]
  set_property -dict [list CONFIG.C_GPIO_WIDTH {32} CONFIG.C_ALL_OUTPUTS {1} CONFIG.C_IS_DUAL {1} \
    CONFIG.C_GPIO2_WIDTH {32} CONFIG.C_ALL_OUTPUTS_2 {1}] $g
  connect_bd_net [get_bd_pins axi_gpio_play_$n/gpio_io_o] [get_bd_pins hrx_player_$n/ctrl]
  connect_bd_net [get_bd_pins axi_gpio_play_$n/gpio2_io_o] [get_bd_pins hrx_player_$n/iq]
  hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /axi_bram_ctrl_$n/S_AXI
  hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /axi_gpio_play_$n/S_AXI
}

# ---- receive --------------------------------------------------------------------------------------------------------
create_bd_cell -type module -reference hrx_capture hrx_capture_0
connect_bd_net [get_bd_pins rfdc/clk_adc2] [get_bd_pins hrx_capture_0/clk]
connect_bd_net [get_bd_pins rst_clk_adc2/peripheral_aresetn] [get_bd_pins hrx_capture_0/aresetn]
connect_bd_net [get_bd_pins zynq_ultra_ps_e_0/pl_clk0] [get_bd_pins hrx_capture_0/status_clk]
connect_bd_intf_net [get_bd_intf_pins rfdc/m20_axis] [get_bd_intf_pins hrx_capture_0/s0_axis]
connect_bd_intf_net [get_bd_intf_pins rfdc/m22_axis] [get_bd_intf_pins hrx_capture_0/s1_axis]
set g [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_gpio axi_gpio_capture]
set_property -dict [list CONFIG.C_GPIO_WIDTH {32} CONFIG.C_ALL_OUTPUTS {1} CONFIG.C_IS_DUAL {1} \
  CONFIG.C_GPIO2_WIDTH {32} CONFIG.C_ALL_INPUTS_2 {1}] $g
connect_bd_net [get_bd_pins axi_gpio_capture/gpio_io_o] [get_bd_pins hrx_capture_0/ctrl]
connect_bd_net [get_bd_pins hrx_capture_0/status] [get_bd_pins axi_gpio_capture/gpio2_io_i]
set fifo [create_bd_cell -type ip -vlnv xilinx.com:ip:axis_data_fifo axis_data_fifo_0]
set_property -dict [list CONFIG.IS_ACLK_ASYNC {1} CONFIG.FIFO_DEPTH {32768} CONFIG.TDATA_NUM_BYTES {16} CONFIG.HAS_TLAST {1}] $fifo
connect_bd_intf_net [get_bd_intf_pins hrx_capture_0/m_axis] [get_bd_intf_pins axis_data_fifo_0/S_AXIS]
set dma [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_dma axi_dma_0]
set_property -dict [list CONFIG.c_include_sg {0} CONFIG.c_include_mm2s {0} CONFIG.c_sg_length_width {26} \
  CONFIG.c_addr_width {40} CONFIG.c_m_axi_s2mm_data_width {128} CONFIG.c_s_axis_s2mm_tdata_width {128} \
  CONFIG.c_s2mm_burst_size {16}] $dma
connect_bd_intf_net [get_bd_intf_pins axis_data_fifo_0/M_AXIS] [get_bd_intf_pins axi_dma_0/S_AXIS_S2MM]
hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /axi_gpio_capture/S_AXI
hrx_axi /zynq_ultra_ps_e_0/M_AXI_HPM0_FPD /axi_dma_0/S_AXI_LITE
hrx_axi /axi_dma_0/M_AXI_S2MM /zynq_ultra_ps_e_0/S_AXI_HP0_FPD
connect_bd_net [get_bd_pins rfdc/clk_adc2] [get_bd_pins axis_data_fifo_0/s_axis_aclk]
connect_bd_net [get_bd_pins rst_clk_adc2/peripheral_aresetn] [get_bd_pins axis_data_fifo_0/s_axis_aresetn]
connect_bd_net [get_bd_pins zynq_ultra_ps_e_0/pl_clk0] [get_bd_pins axis_data_fifo_0/m_axis_aclk]
hrx_concat xlconcat_0 {axi_dma_0/s2mm_introut rfdc/irq} zynq_ultra_ps_e_0/pl_ps_irq0

# The waveform memories whole: the default range would map (and build) only part of them.
foreach {n at} {0 0xA0100000 1 0xA0120000} {
  assign_bd_address -offset $at -range 128K -target_address_space [get_bd_addr_spaces zynq_ultra_ps_e_0/Data] \
    [get_bd_addr_segs axi_bram_ctrl_$n/S_AXI/Mem0]
}
assign_bd_address
validate_bd_design
foreach n {0 1} {
  set depth [get_property CONFIG.MEM_DEPTH [get_bd_cells axi_bram_ctrl_$n]]
  if {$depth != 4096} { error "waveform memory $n is $depth words, not 4096" }
}

hrx_finish rfprj $work $out
