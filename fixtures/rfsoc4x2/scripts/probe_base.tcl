# The vendor's base overlay: the board's LEDs, buttons and switches, the interrupt path to the processor, the PL
# temperature sensor, a MicroBlaze memory, the radio converters' clocks and the PL's DDR4. Raw register accesses only.
proc lite {name} { global base; return $base($name) }

# ---- LEDs, buttons, switches ----------------------------------------------------------------------------------------
foreach {name mask} {leds_gpio 0xF rgbleds_gpio 0x3F} {
  set g [lite $name]
  foreach v [list 0x15 0x2A $mask 0] {
    wr $g [expr {$v & $mask}]
    expect "$name: reads back" [expr {[rd $g] & $mask}] [expr {$v & $mask}]
  }
}
set buttons [rd [lite btns_gpio]]
set switches [rd [lite sws_gpio]]
check "buttons and switches read" [expr {$buttons < 32 && $switches < 16}] [format "(buttons 0x%02x, switches 0x%x)" $buttons $switches]

# ---- interrupts: a GPIO's status set by software, through the controller, to the processor's controller --------------
set intc [lite axi_intc_0]
wr [expr {$intc + 0x1C}] 0
wr [expr {$intc + 0x08}] 0
foreach {name input} {btns_gpio 1 sws_gpio 2} {
  set g [lite $name]
  # Status bits toggle when written with 1: clear whatever is there.
  gpio_ack $g
  wr [expr {$g + 0x128}] 1
  wr [expr {$g + 0x11C}] 0x80000000
  wr [expr {$intc + 0x0C}] 0xFFFFFFFF
  wr [expr {$intc + 0x08}] [expr {1 << $input}]
  wr [expr {$intc + 0x1C}] 3
  check "$name interrupt: quiet before" [expr {![irq_line [irq0 0]]}]
  wr [expr {$g + 0x120}] 1
  check "$name interrupt: reaches the processor (ID [irq0 0])" [wait_irq [irq0 0]]
  expect "$name interrupt: pending input of the controller" [rd [expr {$intc + 0x04}]] [expr {1 << $input}]
  gpio_ack $g
  wr [expr {$intc + 0x0C}] [expr {1 << $input}]
  check "$name interrupt: acknowledged" [expr {![irq_line [irq0 0]]}]
  wr [expr {$g + 0x11C}] 0
  wr [expr {$g + 0x128}] 0
  wr [expr {$intc + 0x08}] 0
}

# ---- temperature and a MicroBlaze program memory ----------------------------------------------------------------------
set code [expr {[rd [expr {[lite system_management_wiz_0] + 0x400}]] & 0xFFFF}]
set celsius [expr {$code * 509.3140064 / 65536.0 - 280.2308787}]
check "PL temperature plausible" [expr {$celsius > 10 && $celsius < 100}] [format "(%.1f C)" $celsius]
foreach n {0 1} {
  set at [expr {[lite iop_pmod${n}_mb_bram_ctrl] + 0xF000}]
  set words {}
  for {set i 0} {$i < 256} {incr i} { lappend words [expr {(0x9E3779B1 * ($i + 1 + $n)) & 0xFFFFFFFF}] }
  write_words $at $words
  check "Pmod $n processor memory" [expr {[read_words $at 256] eq [lmap w $words {format 0x%08x $w}]}]
}

# ---- the radio converters' clocks -----------------------------------------------------------------------------------
set rf [lite radio_rfdc]
check "converter core answers" [expr {[rd $rf] != 0}] [format "(version 0x%08x)" [rd $rf]]
hrx_clocks
foreach {name offset} {"DAC tile 0" 0x4000 "DAC tile 2" 0xC000 "ADC tile 0" 0x14000 "ADC tile 2" 0x1C000} {
  set t [expr {$rf + $offset}]
  wr [expr {$t + 0x04}] 1
  set up [wait_for [expr {$t + 0x0C}] 0xF 0xF 600]
  set status [rd [expr {$t + 0x228}]]
  check "$name: powered up" $up [format "(state %d, status 0x%08x)" [expr {[rd [expr {$t + 0x0C}]] & 0xF}] $status]
  check "$name: PLL locked" [expr {($status & 0x8) != 0}]
}
# With the converters' clocks running, the blocks in their clock domains answer: reset each radio DMA's receive channel.
foreach channel {00 01 20 21} {
  foreach part {real imag} {
    set dma [lite radio_receiver_channel_${channel}_axi_dma_$part]
    wr [expr {$dma + 0x30}] 0x4
    check "receive DMA $channel $part: reset" [wait_for [expr {$dma + 0x30}] 0x4 0x0]
  }
}

# ---- the PL's DDR4 (last: an access hangs if the memory never calibrated) ---------------------------------------------
set ddr [lite ddr4_0]
set words {}
for {set i 0} {$i < 1024} {incr i} { lappend words [expr {(0x7F4A7C15 * ($i + 1)) & 0xFFFFFFFF}] }
foreach {what offset} {"start" 0 "just below 4 GiB" 0xFFFF0000} {
  set at [expr {$ddr + $offset}]
  write_words $at $words
  check "PL DDR4, $what: 4 KiB" [expr {[read_words $at 1024] eq [lmap w $words {format 0x%08x $w}]}]
}
# The design maps 8 GiB; the board's manual says 4 GiB. Report which it is: with 4 GiB the second half is the first again.
mwr -force $ddr 0x11111111
mwr -force [expr {$ddr + 0x100000000}] 0x22222222
set low [rd $ddr]
set high [rd [expr {$ddr + 0x100000000}]]
check "PL DDR4 size" [expr {$high == 0x22222222 && ($low == 0x11111111 || $low == 0x22222222)}] [expr {$low == $high ? "(4 GiB, repeating through the 8 GiB mapped)" : "(8 GiB)"}]
