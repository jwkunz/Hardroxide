# Both DMAs: memory to stream to FIFO to memory, with their interrupts. DMA 1 works above 4 GiB (40-bit addresses).
foreach {n src dst bytes irqs} [list \
  0 0x10000000  0x10100000  65536 [list [irq0 0] [irq0 1]] \
  1 0x800000000 0x840000000 65536 [list [irq1 0] [irq1 1]]] {
  set d $base(axi_dma_$n)
  set words [pattern [expr {1000 + $n}] [expr {$bytes / 4}]]
  write_words $src $words
  write_words $dst [lrepeat [expr {$bytes / 4}] 0xDEADBEEF]
  wr $d 0x4
  check "DMA $n: reset completes" [wait_for $d 0x4 0x0]
  check "DMA $n: interrupt lines low" [expr {![irq_line [lindex $irqs 0]] && ![irq_line [lindex $irqs 1]]}]
  # receive first: run, with the completion interrupt
  wr [expr {$d + 0x30}] 0x1001
  wr [expr {$d + 0x48}] [expr {$dst & 0xFFFFFFFF}]
  wr [expr {$d + 0x4C}] [expr {$dst >> 32}]
  wr [expr {$d + 0x58}] $bytes
  wr $d 0x1001
  wr [expr {$d + 0x18}] [expr {$src & 0xFFFFFFFF}]
  wr [expr {$d + 0x1C}] [expr {$src >> 32}]
  wr [expr {$d + 0x28}] $bytes
  check "DMA $n: both channels finish" [expr {[wait_for [expr {$d + 0x04}] 0x1002 0x1002] && [wait_for [expr {$d + 0x34}] 0x1002 0x1002]}] \
    [format "(MM2S status 0x%08x, S2MM status 0x%08x)" [rd [expr {$d + 0x04}]] [rd [expr {$d + 0x34}]]]
  expect "DMA $n: no error bits" [expr {([rd [expr {$d + 0x04}]] | [rd [expr {$d + 0x34}]]) & 0x770}] 0
  expect "DMA $n: bytes received" [rd [expr {$d + 0x58}]] $bytes
  check "DMA $n: data at [format 0x%x $dst]" [expr {[read_words $dst [expr {$bytes / 4}]] eq $words}]
  check "DMA $n: both interrupt lines high" [expr {[irq_line [lindex $irqs 0]] && [irq_line [lindex $irqs 1]]}]
  wr [expr {$d + 0x04}] 0x7000
  wr [expr {$d + 0x34}] 0x7000
  check "DMA $n: interrupt lines low again" [expr {![irq_line [lindex $irqs 0]] && ![irq_line [lindex $irqs 1]]}]
}
