# Bring an RFSoC 4x2 in JTAG boot mode to a known state from xsdb, and helpers for the probes (probe.py sources this).
# Everything done here is volatile: a power cycle undoes it. Only one JTAG board may be attached.

# Reset the system, program the PL with `bit`, initialise the PS from `psu_init`, open the PS-PL boundary.
# The sequence is the one the vendor's debug launch uses.
proc hrx_bringup {bit psu_init_file} {
  connect
  set parts [llength [jtag targets -target-properties -filter {name == "xczu48dr"}]]
  if {$parts != 1} { error "expected exactly one xczu48dr on JTAG, found $parts" }
  targets -set -nocase -filter {name =~ "APU*"}
  rst -system
  after 3000
  targets -set -nocase -filter {name =~ "PSU"}
  fpga -file $bit
  targets -set -nocase -filter {name =~ "APU*"}
  configparams force-mem-access 1
  uplevel #0 [list source $psu_init_file]
  uplevel #0 {
    psu_init
    after 1000
    psu_post_config
    psu_ps_pl_isolation_removal
    after 1000
    psu_ps_pl_reset_config
  }
  # Take core 0 out of reset and leave it halted: the interrupt controller can only be read through a core.
  targets -set -nocase -filter {name =~ "*A53*#0"}
  rst -processor
  targets -set -nocase -filter {name =~ "PSU"}
}

proc rd {addr} { return [expr {"0x[format %08x [mrd -force -value $addr]]"}] }
proc wr {addr value} { mwr -force $addr $value }

set ::hrx_failures 0
proc check {what ok {detail ""}} {
  if {$ok} {
    puts "PROBE ok    $what $detail"
  } else {
    puts "PROBE FAIL  $what $detail"
    incr ::hrx_failures
  }
}
proc expect {what got want} { check $what [expr {$got == $want}] [format "(got 0x%08x, want 0x%08x)" $got $want] }

# Wait until the bits of `mask` read `want` at `addr`.
proc wait_for {addr mask want {tries 200}} {
  for {set i 0} {$i < $tries} {incr i} {
    if {([rd $addr] & $mask) == $want} { return 1 }
    after 5
  }
  return 0
}

# Is shared interrupt `id`'s line high where it enters the PS's interrupt controller? Read from the GIC-400's line status
# registers (GICD_SPISR): the pending registers do not show a line while the controller is disabled.
proc irq_line {id} {
  targets -set -nocase -filter {name =~ "*A53*#0"}
  set word [rd [expr {0xF9010D04 + 4 * (($id - 32) / 32)}]]
  targets -set -nocase -filter {name =~ "PSU"}
  return [expr {($word >> (($id - 32) % 32)) & 1}]
}
proc wait_irq {id {tries 100}} {
  for {set i 0} {$i < $tries} {incr i} {
    if {[irq_line $id]} { return 1 }
    after 5
  }
  return 0
}
# pl_ps_irq0[n] and pl_ps_irq1[n] as interrupt IDs.
proc irq0 {n} { return [expr {121 + $n}] }
proc irq1 {n} { return [expr {136 + $n}] }

# Stop unless the design maps at least `bytes` for `instance`: a longer write would run into its neighbours.
proc require_room {instance bytes} {
  if {$::size($instance) < $bytes} { error "$instance maps $::size($instance) bytes, the probe needs $bytes" }
}

# Clear an AXI GPIO's interrupt status. Its bits toggle when written with 1, so only the set ones are written.
proc gpio_ack {gpio} { wr [expr {$gpio + 0x120}] [rd [expr {$gpio + 0x120}]] }

# A repeatable word pattern.
proc pattern {seed n} {
  set out {}
  set x $seed
  for {set i 0} {$i < $n} {incr i} {
    set x [expr {($x * 1103515245 + 12345) & 0xFFFFFFFF}]
    lappend out [format 0x%08x $x]
  }
  return $out
}
proc write_words {addr words} { mwr -force $addr $words [llength $words] }
proc read_words {addr n} {
  set out {}
  foreach v [mrd -force -value $addr $n] { lappend out [format 0x%08x $v] }
  return $out
}
