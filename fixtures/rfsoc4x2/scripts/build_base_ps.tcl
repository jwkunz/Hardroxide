# A processor-only design with the board preset and the base overlay's PS-to-PL settings, built for one thing: its
# psu_init. The base overlay as PYNQ ships it is a bitstream and a hardware handoff, without the start-up code that
# `hardroxide run` needs. The overlay's own processor settings are not used: PYNQ never applies them (its boot loader
# initialises the processor from the board's settings and loading the overlay changes only the PS-to-PL side), and they
# describe a different memory from the one the preset configures. The settings below are read from the overlay's
# base.hwh.
# Usage: vivado -mode batch -source build_base_ps.tcl -tclargs <workdir> <out.xsa>     (needs no bitstream license)
source [file join [file dirname [info script]] common.tcl]
set work [lindex $argv 0]
set out [lindex $argv 1]

set ps [hrx_project basepsprj $work {
  PSU__USE__M_AXI_GP0 1 PSU__USE__M_AXI_GP1 1 PSU__USE__M_AXI_GP2 1
  PSU__USE__S_AXI_GP2 1 PSU__USE__S_AXI_GP4 1 PSU__USE__S_AXI_GP6 1
  PSU__MAXIGP0__DATA_WIDTH 128 PSU__MAXIGP1__DATA_WIDTH 128 PSU__MAXIGP2__DATA_WIDTH 32
  PSU__SAXIGP2__DATA_WIDTH 128 PSU__SAXIGP4__DATA_WIDTH 128 PSU__SAXIGP6__DATA_WIDTH 128
  PSU__USE__IRQ0 1
  PSU__FPGA_PL1_ENABLE 1 PSU__CRL_APB__PL1_REF_CTRL__SRCSEL IOPLL PSU__CRL_APB__PL1_REF_CTRL__FREQMHZ 300
  PSU__GPIO_EMIO__PERIPHERAL__ENABLE 1 PSU__GPIO_EMIO__PERIPHERAL__IO 95
  PSU__NUM_FABRIC_RESETS 1
}]
# Nothing is attached to the PL ports; their clocks only need a source for the design to generate.
foreach name {maxihpm0_fpd maxihpm1_fpd maxihpm0_lpd saxihp0_fpd saxihp2_fpd saxi_lpd} {
  hrx_clock zynq_ultra_ps_e_0/${name}_aclk
}
validate_bd_design
save_bd_design
generate_target all [get_files design_1.bd]
write_hw_platform -fixed -force $out
foreach {clock want} {PL0 100 PL1 300} {
  set got [get_property CONFIG.PSU__CRL_APB__${clock}_REF_CTRL__ACT_FREQMHZ $ps]
  if {abs($got - $want) > 0.01} { error "$clock is $got MHz, the base overlay needs $want" }
}
puts "hrx: wrote $out"
