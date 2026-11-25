# The `scale` kernel: it reads and writes DDR itself through HP0, with 64-bit address registers; done interrupt on
# pl_ps_irq0[0]. Run once in low DDR and once above 4 GiB. Offsets from the HLS driver header.
set k $base(scale_0)
expect "kernel idle" [expr {[rd $k] & 0x4}] 0x4
wr [expr {$k + 0x04}] 1
wr [expr {$k + 0x08}] 1
set n 4096
foreach {src dst factor} {0x10000000 0x10100000 3 0x800000000 0x840000000 7} {
  set in [pattern [expr {$factor * 11}] $n]
  write_words $src $in
  write_words $dst [lrepeat $n 0xDEADBEEF]
  foreach {offset value} [list 0x10 $src 0x1C $dst] {
    wr [expr {$k + $offset}] [expr {$value & 0xFFFFFFFF}]
    wr [expr {$k + $offset + 4}] [expr {$value >> 32}]
  }
  wr [expr {$k + 0x28}] $n
  wr [expr {$k + 0x30}] $factor
  wr $k 1
  set at [format 0x%x $src]
  check "scale at $at finishes" [wait_irq [irq0 0]]
  set want [lmap w $in {format 0x%08x [expr {($w * $factor) & 0xFFFFFFFF}]}]
  check "scale at $at: $n results" [expr {[read_words $dst $n] eq $want}]
  wr [expr {$k + 0x0C}] 1
  check "interrupt acknowledged" [expr {![irq_line [irq0 0]]}]
}
