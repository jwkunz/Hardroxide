# Builds fixtures/pynq_z2_loopback.xsa: PS7 + two AXI UART Lite + two AXI IIC.
# The pairs are wired to each other INSIDE the fabric (the custom top level
# below replaces the generated wrapper's external ports), so on-board tests
# need no pins and no jumpers:
#   UART0.tx -> UART1.rx and UART1.tx -> UART0.rx
#   IIC0 and IIC1 share one open-drain SDA and one SCL line (wired-AND)
# Interrupts via xlconcat to IRQ_F2P[3:0].
# Usage: vivado -mode batch -source build_pynq_z2_loopback.tcl -tclargs <workdir> <out.xsa>
set work [lindex $argv 0]
set out  [lindex $argv 1]
create_project -force lbprj $work -part xc7z020clg400-1
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

# Board-accurate PS configuration (DDR, MIO, UART, ...): see make_board_scripts.py
source [file join [file dirname [info script]] pynq_z2_ps7_board.tcl]
apply_pynq_z2_ps7 $ps
foreach n {0 1} {
  set u [create_bd_cell -type ip -vlnv xilinx.com:ip:axi_uartlite axi_uartlite_$n]
  set_property -dict [list CONFIG.C_BAUDRATE {115200}] $u
  create_bd_cell -type ip -vlnv xilinx.com:ip:axi_iic axi_iic_$n
}
set cc [create_bd_cell -type ip -vlnv xilinx.com:ip:xlconcat xlconcat_0]
set_property CONFIG.NUM_PORTS {4} $cc
connect_bd_net [get_bd_pins axi_uartlite_0/interrupt] [get_bd_pins xlconcat_0/In0]
connect_bd_net [get_bd_pins axi_uartlite_1/interrupt] [get_bd_pins xlconcat_0/In1]
connect_bd_net [get_bd_pins axi_iic_0/iic2intc_irpt] [get_bd_pins xlconcat_0/In2]
connect_bd_net [get_bd_pins axi_iic_1/iic2intc_irpt] [get_bd_pins xlconcat_0/In3]
connect_bd_net [get_bd_pins xlconcat_0/dout] [get_bd_pins processing_system7_0/IRQ_F2P]
foreach ip {axi_uartlite_0 axi_uartlite_1 axi_iic_0 axi_iic_1} {
  apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config { \
    Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master {/processing_system7_0/M_AXI_GP0} \
    Slave "/$ip/S_AXI" intc_ip {New AXI Interconnect} master_apm {0}} [get_bd_intf_pins $ip/S_AXI]
}
foreach n {0 1} {
  make_bd_intf_pins_external [get_bd_intf_pins axi_uartlite_$n/UART]
  make_bd_intf_pins_external [get_bd_intf_pins axi_iic_$n/IIC]
}
assign_bd_address
validate_bd_design
save_bd_design
make_wrapper -files [get_files design_1.bd] -top
set wrapper [glob $work/lbprj.gen/sources_1/bd/design_1/hdl/design_1_wrapper.v]
# Replace the generated wrapper (which would expose the interfaces as pins)
# with a portless top that wires the interfaces to each other. An I2C line is
# open-drain: it is low when any master drives it low (t=0 and o=0).
set loop $work/loopback_top.v
set f [open $loop w]
puts $f {`timescale 1 ps / 1 ps
module design_1_wrapper ();
  wire UART_0_rxd, UART_0_txd, UART_1_rxd, UART_1_txd;
  wire IIC_0_scl_i, IIC_0_scl_o, IIC_0_scl_t, IIC_0_sda_i, IIC_0_sda_o, IIC_0_sda_t;
  wire IIC_1_scl_i, IIC_1_scl_o, IIC_1_scl_t, IIC_1_sda_i, IIC_1_sda_o, IIC_1_sda_t;
  assign UART_1_rxd = UART_0_txd;
  assign UART_0_rxd = UART_1_txd;
  wire scl_line = (IIC_0_scl_t | IIC_0_scl_o) & (IIC_1_scl_t | IIC_1_scl_o);
  wire sda_line = (IIC_0_sda_t | IIC_0_sda_o) & (IIC_1_sda_t | IIC_1_sda_o);
  assign IIC_0_scl_i = scl_line;
  assign IIC_1_scl_i = scl_line;
  assign IIC_0_sda_i = sda_line;
  assign IIC_1_sda_i = sda_line;
  design_1 design_1_i
       (.IIC_0_scl_i(IIC_0_scl_i), .IIC_0_scl_o(IIC_0_scl_o), .IIC_0_scl_t(IIC_0_scl_t),
        .IIC_0_sda_i(IIC_0_sda_i), .IIC_0_sda_o(IIC_0_sda_o), .IIC_0_sda_t(IIC_0_sda_t),
        .IIC_1_scl_i(IIC_1_scl_i), .IIC_1_scl_o(IIC_1_scl_o), .IIC_1_scl_t(IIC_1_scl_t),
        .IIC_1_sda_i(IIC_1_sda_i), .IIC_1_sda_o(IIC_1_sda_o), .IIC_1_sda_t(IIC_1_sda_t),
        .UART_0_rxd(UART_0_rxd), .UART_0_txd(UART_0_txd),
        .UART_1_rxd(UART_1_rxd), .UART_1_txd(UART_1_txd));
endmodule}
close $f
add_files -norecurse $loop
set_property top design_1_wrapper [current_fileset]
launch_runs impl_1 -to_step write_bitstream -jobs 8
wait_on_run impl_1
write_hw_platform -fixed -include_bit -force $out
