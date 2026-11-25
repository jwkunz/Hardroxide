# GPIO interrupt (raised by the second GPIO's output) and timer interrupt, through the AXI INTC to pl_ps_irq0[0].
set timer $base(axi_timer_0)
set iic $base(axi_iic_0)
set gin $base(axi_gpio_0)
set gout $base(axi_gpio_1)
set intc $base(axi_intc_0)

wr [expr {$gout}] 0
expect "GPIO loop reads 0" [rd $gin] 0
wr $gout 0x9
expect "GPIO loop reads 9" [rd $gin] 9
expect "I2C controller idle (status: both FIFOs empty)" [expr {[rd [expr {$iic + 0x104}]] & 0xC4}] 0xC0

# Controller off: clear everything, then enable all three inputs.
gpio_ack $gin
wr [expr {$intc + 0x0C}] 0xFFFFFFFF
wr [expr {$intc + 0x08}] 0x7
wr [expr {$intc + 0x1C}] 0x3
check "interrupt line starts low" [expr {![irq_line [irq0 0]]}]

wr [expr {$gin + 0x128}] 1
wr [expr {$gin + 0x11C}] 0x80000000
wr $gout 0x6
check "GPIO interrupt reaches the controller (input 2)" [wait_for [expr {$intc + 0x04}] 0x4 0x4]
check "and the PS (pl_ps_irq0\[0\])" [irq_line [irq0 0]]
gpio_ack $gin
wr [expr {$intc + 0x0C}] 0x4
check "acknowledged: line low again" [expr {![irq_line [irq0 0]]}]

# Timer 0: count up from near the top, interrupt on overflow.
wr [expr {$timer + 0x04}] 0xFFF00000
wr $timer 0x20
wr $timer 0xC0
check "timer interrupt reaches the controller (input 0)" [wait_for [expr {$intc + 0x04}] 0x1 0x1]
check "and the PS" [irq_line [irq0 0]]
wr $timer 0x100
wr [expr {$intc + 0x0C}] 0x1
check "acknowledged: line low again" [expr {![irq_line [irq0 0]]}]
