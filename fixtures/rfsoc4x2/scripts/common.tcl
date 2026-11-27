# Shared steps of the RFSoC 4x2 fixture builds (ADR 0005). Every design starts from the vendor's board preset
# (board_files/, from Real Digital's RFSoC4x2-BSP), so each XSA's psu_init configures the real board.

set hrx_dir [file dirname [file normalize [info script]]]
set_param board.repoPaths [list [file normalize $hrx_dir/../board_files]]
set hrx_part xczu48dr-ffvg1517-2-e

# A project with a block design `design_1` holding the processing system as the board preset configures it, with
# pl_clk0 at 100 MHz. `ps_settings` are further PS options (which AXI ports and interrupt inputs to use).
proc hrx_project {name work ps_settings {ip_repo ""}} {
  global hrx_part
  create_project -force $name $work -part $hrx_part
  set_property board_part realdigital.org:rfsoc4x2:part0:1.0 [current_project]
  if {$ip_repo ne ""} {
    set_property ip_repo_paths $ip_repo [current_project]
    update_ip_catalog
  }
  create_bd_design design_1
  set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:zynq_ultra_ps_e zynq_ultra_ps_e_0]
  apply_bd_automation -rule xilinx.com:bd_rule:zynq_ultra_ps_e -config {apply_board_preset "1"} $ps
  # The preset turns some PL ports on: start from none, then take what the design asks for.
  set base {
    PSU__USE__M_AXI_GP0 0 PSU__USE__M_AXI_GP1 0 PSU__USE__M_AXI_GP2 0
    PSU__USE__S_AXI_GP2 0 PSU__USE__S_AXI_GP3 0 PSU__USE__IRQ0 0 PSU__USE__IRQ1 0
    PSU__FPGA_PL0_ENABLE 1 PSU__CRL_APB__PL0_REF_CTRL__FREQMHZ 100
  }
  foreach {k v} [concat $base $ps_settings] { set_property CONFIG.$k $v $ps }
  return $ps
}

# Connect an AXI slave to a master through a SmartConnect (the master's port decides which SmartConnect).
proc hrx_axi {master slave} {
  apply_bd_automation -rule xilinx.com:bd_rule:axi4 -config [list \
    Clk_master {Auto} Clk_slave {Auto} Clk_xbar {Auto} Master $master Slave $slave \
    ddr_seg {Auto} intc_ip {Auto} master_apm {0}] [get_bd_intf_pins $slave]
}

# Several AXI masters into one slave port of the processing system, through one SmartConnect on pl_clk0. (Connection
# automation leaves a dangling interconnect when a second master asks for a port that is already connected.) `masters`
# is a list of {interface clock_pin} pairs. Call it after a first hrx_axi, which creates the pl_clk0 reset.
proc hrx_axi_masters {name masters slave slave_clk} {
  set clk [get_bd_pins zynq_ultra_ps_e_0/pl_clk0]
  set smc [create_bd_cell -type ip -vlnv xilinx.com:ip:smartconnect $name]
  set_property -dict [list CONFIG.NUM_SI [expr {[llength $masters] / 2}] CONFIG.NUM_MI 1] $smc
  set reset [lindex [get_bd_cells -filter {VLNV =~ *proc_sys_reset*}] 0]
  connect_bd_net [get_bd_pins $reset/peripheral_aresetn] [get_bd_pins $name/aresetn]
  foreach pin [list $name/aclk $slave_clk] { hrx_clock $pin }
  set i 0
  foreach {master master_clk} $masters {
    connect_bd_intf_net [get_bd_intf_pins $master] [get_bd_intf_pins $name/[format S%02d_AXI $i]]
    hrx_clock $master_clk
    incr i
  }
  connect_bd_intf_net [get_bd_intf_pins $name/M00_AXI] [get_bd_intf_pins $slave]
}

# Clock `pin` from pl_clk0 unless it already has a clock.
proc hrx_clock {pin} {
  if {[get_bd_nets -quiet -of_objects [get_bd_pins $pin]] eq ""} {
    connect_bd_net [get_bd_pins zynq_ultra_ps_e_0/pl_clk0] [get_bd_pins $pin]
  }
}

# Concatenate interrupt (or other one-bit) pins, first at bit 0, into `sink`.
proc hrx_concat {name sources sink} {
  set cc [create_bd_cell -type ip -vlnv xilinx.com:ip:xlconcat $name]
  set_property CONFIG.NUM_PORTS [llength $sources] $cc
  set i 0
  foreach s $sources { connect_bd_net [get_bd_pins $s] [get_bd_pins $name/In$i]; incr i }
  connect_bd_net [get_bd_pins $name/dout] [get_bd_pins $sink]
}

# The open design's worst setup, hold and pulse width slacks and its failing endpoints, from the report's summary:
# {wns whs wpws setup_failing hold_failing pulse_failing}.
proc hrx_timing {} {
  set report [report_timing_summary -no_header -return_string]
  # The "Design Timing Summary" table: WNS TNS fail total WHS THS fail total WPWS TPWS fail total.
  if {![regexp {WNS\(ns\)[^\n]*\n[ -]*\n\s*([^\n]+)} $report -> row]} { error "no timing summary" }
  lassign [regexp -all -inline {\S+} $row] wns _ tnsfail _ whs _ thsfail _ wpws _ tpwsfail _
  return [list $wns $whs $wpws $tnsfail $thsfail $tpwsfail]
}

# Re-analyse the open, routed design at a speed grade (e.g. 1), print the result and return whether it meets timing.
# Any clock uncertainty set to give the build a margin is removed first (no constraint here uses it otherwise).
proc hrx_signoff {name grade} {
  set_clock_uncertainty 0 [get_clocks]
  set_speed_grade -$grade
  lassign [hrx_timing] wns whs wpws setup hold pulse
  set ok [expr {$wns >= 0 && $whs >= 0 && $wpws >= 0}]
  puts "hrx: timing $name speed -$grade: WNS $wns WHS $whs WPWS $wpws ns, failing endpoints $setup/$hold/$pulse: [expr {$ok ? "met" : "NOT MET"}]"
  return $ok
}

# Validate, build and export. `top` replaces the generated wrapper when given (a file defining design_1_wrapper);
# `sources` are further HDL or constraint files. Fails if the design does not meet timing at the part's speed grade (-2)
# or at -1, which the board's reference manual lists (see ../README.md). A design that needs it for -1 sets `hrx_margins`
# first, a list of {clock -setup|-hold ns}: extra slack asked of placement and routing, not counted in the check.
proc hrx_finish {name work out {top ""} {sources {}}} {
  assign_bd_address
  validate_bd_design
  save_bd_design
  if {$top eq ""} {
    make_wrapper -files [get_files design_1.bd] -top
    add_files -norecurse [glob $work/$name.gen/sources_1/bd/design_1/hdl/design_1_wrapper.v]
  } else {
    add_files -norecurse $top
  }
  set xdc $work/bitstream.xdc
  set f [open $xdc w]
  puts $f {set_property BITSTREAM.GENERAL.COMPRESS TRUE [current_design]}
  puts $f {set_property BITSTREAM.CONFIG.OVERTEMPSHUTDOWN ENABLE [current_design]}
  puts $f {set_property BITSTREAM.CONFIG.UNUSEDPIN PULLUP [current_design]}
  close $f
  add_files -fileset constrs_1 $xdc
  foreach s $sources {
    if {[file extension $s] eq ".xdc"} { add_files -fileset constrs_1 $s } else { add_files -norecurse $s }
  }
  set_property top design_1_wrapper [current_fileset]
  update_compile_order -fileset sources_1
  global hrx_margins
  if {[info exists hrx_margins]} {
    set f [open $work/margins.tcl w]
    foreach {clock kind ns} $hrx_margins { puts $f "set_clock_uncertainty $kind $ns \[get_clocks $clock\]" }
    close $f
    add_files -fileset utils_1 -norecurse $work/margins.tcl
    set_property STEPS.PLACE_DESIGN.TCL.PRE [get_files $work/margins.tcl] [get_runs impl_1]
  }
  launch_runs impl_1 -to_step write_bitstream -jobs 6
  wait_on_run impl_1
  set run [get_runs impl_1]
  if {[get_property PROGRESS $run] ne "100%"} { error "implementation failed: [get_property STATUS $run]" }
  open_run impl_1
  set grade [string trimleft [lindex [split [get_property PART [current_design]] -] end-1] -]
  set failed {}
  foreach g [list $grade 1] { if {![hrx_signoff $name $g]} { lappend failed -$g } }
  close_design
  if {[llength $failed]} { error "timing not met at speed grade [join $failed {, }]" }
  write_hw_platform -fixed -include_bit -force $out
  puts "hrx: wrote $out"
}
