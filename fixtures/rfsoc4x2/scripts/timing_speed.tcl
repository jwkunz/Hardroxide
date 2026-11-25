# Would a routed design meet timing on another speed grade? Opens routed checkpoints and re-analyses each with that
# grade's delays, without placing or routing again. (The designs cannot simply be built for -1: Vivado rejects the board
# preset's processor clocks and memory timing for it.)
# Usage: vivado -mode batch -source timing_speed.tcl -tclargs <speed grade, e.g. -1> <routed.dcp>...
set grade [lindex $argv 0]
set failed 0
foreach dcp [lrange $argv 1 end] {
  open_checkpoint $dcp
  set part [get_property PART [current_design]]
  foreach g [list [lindex [split $part -] end-1] [string trimleft $grade -]] {
    set_speed_grade -$g
    set report [report_timing_summary -no_header -return_string]
    # The "Design Timing Summary" table: WNS TNS fail total WHS THS fail total WPWS TPWS fail total.
    if {![regexp {WNS\(ns\)[^\n]*\n[ -]*\n\s*([^\n]+)} $report -> row]} { error "no timing summary for $dcp" }
    lassign [regexp -all -inline {\S+} $row] wns tns tnsfail _ whs ths thsfail _ wpws tpws tpwsfail _
    set ok [expr {$wns >= 0 && $whs >= 0 && $wpws >= 0}]
    puts "hrx: [file tail [file dirname [file dirname [file dirname [file dirname $dcp]]]]] speed -$g: WNS $wns WHS $whs WPWS $wpws ns, failing endpoints $tnsfail/$thsfail/$tpwsfail: [expr {$ok ? "met" : "NOT MET"}]"
    if {!$ok && -$g eq $grade} { incr failed }
  }
  close_design
}
puts "hrx: $failed design(s) do not meet timing at speed grade $grade"
exit [expr {$failed != 0}]
