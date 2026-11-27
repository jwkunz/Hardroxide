# Every peripheral against its partner inside the FPGA, through HPM0_LPD, with the interrupts on pl_ps_irq0[0..7]:
# timer, uart0, uart1, iic0, iic1, spi, gpio0, fifo.
set timer $base(axi_timer_0)
set u0 $base(axi_uartlite_0)
set u1 $base(axi_uartlite_1)
set i0 $base(axi_iic_0)
set i1 $base(axi_iic_1)
set spi $base(axi_quad_spi_0)
set gin $base(axi_gpio_0)
set gout $base(axi_gpio_1)
set fifo $base(axi_fifo_mm_s_0)
set bram $base(axi_bram_ctrl_0)

# GPIO and its interrupt.
wr $gout 0
gpio_ack $gin
wr [expr {$gin + 0x128}] 1
wr [expr {$gin + 0x11C}] 0x80000000
check "GPIO interrupt idle" [expr {![irq_line [irq0 6]]}]
wr $gout 0xA5
expect "GPIO loop" [rd $gin] 0xA5
check "GPIO interrupt (irq0\[6\])" [irq_line [irq0 6]]
gpio_ack $gin

# Block RAM: all 64 KiB.
set p [pattern 7 16384]
require_room axi_bram_ctrl_0 65536
write_words $bram $p
check "block RAM, 64 KiB" [expr {[read_words $bram 16384] eq $p}]

# Timer.
wr [expr {$timer + 0x04}] 0xFFF00000
wr $timer 0x20
wr $timer 0xC0
check "timer interrupt (irq0\[0\])" [wait_irq [irq0 0]]
wr $timer 0x100

# UARTs, both directions.
foreach {from to fn tn irq} [list $u0 $u1 0 1 2 $u1 $u0 1 0 1] {
  wr [expr {$from + 0x0C}] 0x3
  wr [expr {$to + 0x0C}] 0x13
  wr [expr {$from + 0x04}] [expr {0x41 + $fn}]
  check "UART $fn to UART $tn: byte arrives" [wait_for [expr {$to + 0x08}] 0x1 0x1]
  expect "UART $fn to UART $tn: byte" [rd $to] [expr {0x41 + $fn}]
}
# Interrupt lines of the UARTs are pulses; their delivery is checked by the drivers later.

# I2C: controller 1 listens at address 0x55, controller 0 writes two bytes to it.
foreach c [list $i0 $i1] { wr [expr {$c + 0x40}] 0xA; wr [expr {$c + 0x120}] 0xF }
wr [expr {$i1 + 0x110}] [expr {0x55 << 1}]
wr [expr {$i1 + 0x100}] 0x1
wr [expr {$i0 + 0x100}] 0x1
wr [expr {$i0 + 0x108}] [expr {0x100 | (0x55 << 1)}]
wr [expr {$i0 + 0x108}] 0xDE
wr [expr {$i0 + 0x108}] [expr {0x200 | 0xAD}]
check "I2C: bytes arrive" [wait_for [expr {$i1 + 0x104}] 0x40 0x0]
expect "I2C: first byte" [expr {[rd [expr {$i1 + 0x10C}]] & 0xFF}] 0xDE
wait_for [expr {$i1 + 0x104}] 0x40 0x0
expect "I2C: second byte" [expr {[rd [expr {$i1 + 0x10C}]] & 0xFF}] 0xAD

# SPI: master, manual slave select, one byte out and (looped) back in.
wr [expr {$spi + 0x40}] 0xA
wr [expr {$spi + 0x60}] 0x1E6
wr [expr {$spi + 0x70}] 0xFFFFFFFE
wr [expr {$spi + 0x68}] 0x5C
wr [expr {$spi + 0x60}] 0x086
check "SPI: transfer completes" [wait_for [expr {$spi + 0x64}] 0x1 0x0]
expect "SPI: byte comes back" [expr {[rd [expr {$spi + 0x6C}]] & 0xFF}] 0x5C

# Stream FIFO: transmit eight words, receive them.
wr [expr {$fifo + 0x18}] 0xA5
wr [expr {$fifo + 0x08}] 0xA5
after 10
wr $fifo 0xFFFFFFFF
set words [pattern 99 8]
foreach w $words { wr [expr {$fifo + 0x10}] $w }
wr [expr {$fifo + 0x14}] 32
check "FIFO: packet received" [wait_for [expr {$fifo + 0x1C}] 0xFFFF 8]
expect "FIFO: length" [rd [expr {$fifo + 0x24}]] 32
set got {}
for {set i 0} {$i < 8} {incr i} { lappend got [format 0x%08x [rd [expr {$fifo + 0x20}]]] }
check "FIFO: data" [expr {$got eq $words}]
wr [expr {$fifo + 0x04}] 0x04000000
check "FIFO interrupt enabled, line high (irq0\[7\])" [expr {([rd $fifo] != 0) && [irq_line [irq0 7]]}]
