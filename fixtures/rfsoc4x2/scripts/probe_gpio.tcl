# The processor system the board preset configures (DDR in both address ranges), then the LEDs, buttons and switches.
foreach {name at} {"low DDR, start" 0x00100000 "low DDR, end" 0x7FF00000 "high DDR, start" 0x800000000 "high DDR, end" 0x87FF00000} {
  set p [pattern [expr {$at & 0xFFFFFF | 5}] 1024]
  write_words $at $p
  check "$name: 4 KiB read back" [expr {[read_words $at 1024] eq $p}]
}
# Address lines: a distinct word at each power of two, all read after all are written.
set spots {}
for {set bit 12} {$bit < 31} {incr bit} { lappend spots [expr {1 << $bit}] }
lappend spots 0x800000000 0x840000000
foreach a $spots { wr $a [expr {($a >> 4) & 0xFFFFFFFF ^ 0x5A5A0000}] }
set bad {}
foreach a $spots { if {[rd $a] != (($a >> 4) & 0xFFFFFFFF ^ 0x5A5A0000)} { lappend bad [format 0x%x $a] } }
check "DDR address lines do not alias" [expr {$bad eq {}}] $bad

set g0 $base(axi_gpio_0)
set g1 $base(axi_gpio_1)
foreach v {0x5 0xA 0xF} {
  wr $g0 $v
  expect "LED register" [rd $g0] $v
}
wr $g1 0x21
expect "RGB register" [rd $g1] 0x21
set inputs [rd [expr {$g0 + 8}]]
check "buttons and switches read" [expr {$inputs < 512}] [format "(0x%03x: buttons 0x%02x, switches 0x%x)" $inputs [expr {$inputs & 0x1F}] [expr {$inputs >> 5}]]
puts "  LEDs: all four white LEDs on, RGB0 red, RGB1 blue"
wr $g0 0xF
