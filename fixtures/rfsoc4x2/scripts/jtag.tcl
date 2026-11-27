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

# Program the board's RF clock chips (the LMK04828, then the two LMX2594) over the processor's SPI 0, from the register
# files in ../clocks. Volatile.
proc hrx_clocks {} {
  global scripts
  set spi 0xFF040000
  set gpio 0xFF0A0000
  set lmk [hrx_register_file $scripts/../clocks/LMK04828_245.76.txt]
  set lmx [hrx_register_file $scripts/../clocks/LMX2594_491.52.txt]
  check "clock register files read" [expr {[llength $lmk] == 136 && [llength $lmx] == 113}] "([llength $lmk], [llength $lmx])"
  # MIO 8 and 12: the LMK's reference select (0, 0 = the on-board oscillator). MIO 7 is wired to its reset pin: held
  # low and never driven high, because the register file turns the chip's pin into an output held low (the chip is
  # reset over SPI, by the file's first line). Floating, the processor pin's pull-up would hold the chip in reset.
  set pins [expr {(1 << 7) | (1 << 8) | (1 << 12)}]
  # A level written to an input is ignored: direction first, then the level, then the driver.
  wr [expr {$gpio + 0x204}] [expr {[rd [expr {$gpio + 0x204}]] | $pins}]
  wr [expr {$gpio + 0x40}] [expr {[rd [expr {$gpio + 0x40}]] & ~$pins}]
  wr [expr {$gpio + 0x208}] [expr {[rd [expr {$gpio + 0x208}]] | $pins}]

  wr [expr {$spi + 0x14}] 0
  wr [expr {$spi + 0x08}] 0x7F
  wr [expr {$spi + 0x04}] 0x7F
  wr [expr {$spi + 0x28}] 1
  wr [expr {$spi + 0x2C}] 1
  wr $spi [expr {0x1 | (7 << 3) | (1 << 14) | (0xF << 10)}]
  wr [expr {$spi + 0x14}] 1
  foreach word $lmk { hrx_spi_write 0 $word }
  after 100
  foreach cs {1 2} {
    hrx_spi_write $cs 0x020000
    hrx_spi_write $cs 0x000000
    foreach word $lmx { hrx_spi_write $cs $word }
    after 20
    hrx_spi_write $cs [lindex $lmx end]
  }
  check "clock chips written" 1
  after 200
}
# One 24-bit write with SPI 0's chip select `cs` held low for all three bytes.
proc hrx_spi_write {cs word} {
  set spi 0xFF040000
  # master, manual chip select, reference clock / 256
  set cr [expr {0x1 | (7 << 3) | (1 << 14)}]
  set select [lindex {0xE 0xD 0xB} $cs]
  wr $spi [expr {$cr | ($select << 10)}]
  foreach shift {16 8 0} { wr [expr {$spi + 0x1C}] [expr {($word >> $shift) & 0xFF}] }
  # a byte received for each byte sent: the transfer is over when all three are in
  for {set i 0} {$i < 3} {incr i} {
    if {![wait_for [expr {$spi + 0x04}] 0x10 0x10]} { error "SPI 0 does not transfer" }
    rd [expr {$spi + 0x20}]
  }
  wr $spi [expr {$cr | (0xF << 10)}]
}
proc hrx_register_file {path} {
  set f [open $path]
  set out {}
  foreach line [split [read $f] \n] { if {[regexp {0x[0-9A-Fa-f]+} $line v]} { lappend out $v } }
  close $f
  return $out
}

