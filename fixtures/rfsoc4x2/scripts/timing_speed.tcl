# Would a routed design meet timing on another speed grade? Opens routed checkpoints and re-analyses each with that
# grade's delays, without placing or routing again. (The designs cannot simply be built for -1: Vivado rejects the board
# preset's processor clocks and memory timing for it.) Builds run the same check for -2 and -1 (common.tcl).
# Usage: vivado -mode batch -source timing_speed.tcl -tclargs <speed grade, e.g. -1> <routed.dcp>...
source [file join [file dirname [info script]] common.tcl]
set grade [string trimleft [lindex $argv 0] -]
set failed 0
foreach dcp [lrange $argv 1 end] {
  open_checkpoint $dcp
  set name [file tail [file dirname [file dirname [file dirname [file dirname $dcp]]]]]
  if {![hrx_signoff $name $grade]} { incr failed }
  close_design
}
puts "hrx: $failed design(s) do not meet timing at speed grade -$grade"
exit [expr {$failed != 0}]
