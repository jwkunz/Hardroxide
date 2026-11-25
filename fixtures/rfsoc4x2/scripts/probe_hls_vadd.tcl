# The `vadd` kernel over AXI-Lite, with its done interrupt on pl_ps_irq0[0]. Offsets from the HLS driver header.
set k $base(vadd_0)
expect "kernel idle" [expr {[rd $k] & 0x4}] 0x4
check "interrupt line low" [expr {![irq_line [irq0 0]]}]
wr [expr {$k + 0x04}] 1
wr [expr {$k + 0x08}] 1
foreach {a b} {2 3 100000 234567 0x7FFFFFF0 0x20} {
  wr [expr {$k + 0x10}] $a
  wr [expr {$k + 0x18}] $b
  wr $k 1
  check "vadd($a, $b) finishes" [wait_irq [irq0 0]]
  expect "vadd($a, $b)" [rd [expr {$k + 0x20}]] [expr {($a + $b) & 0xFFFFFFFF}]
  wr [expr {$k + 0x0C}] 1
  check "interrupt acknowledged" [expr {![irq_line [irq0 0]]}]
}
