# The board's clock chips, the data converter's tiles, the waveform players and the capture path. With SMA cables from
# the DAC outputs to the ADC inputs, the captured tones show which output reaches which input (rf_analyse.py, run by
# probe.py). Each DAC plays its own frequency at half of full scale.
set rf $base(rfdc)
set spi 0xFF040000
set gpio 0xFF0A0000

# ---- clock chips on SPI 0 -------------------------------------------------------------------------------------------
# One 24-bit write with chip select `cs` held low for all three bytes.
proc spi_write {cs word} {
  global spi
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
proc register_file {path} {
  set f [open $path]
  set out {}
  foreach line [split [read $f] \n] { if {[regexp {0x[0-9A-Fa-f]+} $line v]} { lappend out $v } }
  close $f
  return $out
}
set lmk [register_file $scripts/../clocks/LMK04828_245.76.txt]
set lmx [register_file $scripts/../clocks/LMX2594_491.52.txt]
check "clock register files read" [expr {[llength $lmk] == 136 && [llength $lmx] == 113}] "([llength $lmk], [llength $lmx])"

# MIO 7: LMK reset; MIO 8 and 12: its reference select (0, 0 = the on-board oscillator).
set pins [expr {(1 << 7) | (1 << 8) | (1 << 12)}]
foreach reg {0x204 0x208} { wr [expr {$gpio + $reg}] [expr {[rd [expr {$gpio + $reg}]] | $pins}] }
set data [expr {[rd [expr {$gpio + 0x40}]] & ~$pins}]
wr [expr {$gpio + 0x40}] [expr {$data | (1 << 7)}]
after 10
wr [expr {$gpio + 0x40}] $data
after 10

wr [expr {$spi + 0x14}] 0
wr [expr {$spi + 0x08}] 0x7F
wr [expr {$spi + 0x04}] 0x7F
wr [expr {$spi + 0x28}] 1
wr [expr {$spi + 0x2C}] 1
wr $spi [expr {0x1 | (7 << 3) | (1 << 14) | (0xF << 10)}]
wr [expr {$spi + 0x14}] 1
foreach word $lmk { spi_write 0 $word }
after 100
foreach cs {1 2} {
  spi_write $cs 0x020000
  spi_write $cs 0x000000
  foreach word $lmx { spi_write $cs $word }
  after 20
  spi_write $cs [lindex $lmx end]
}
check "clock chips written" 1
after 200

# ---- data converter tiles -------------------------------------------------------------------------------------------
foreach {name offset} {"DAC tile 0" 0x4000 "DAC tile 2" 0xC000 "ADC tile 2" 0x1C000} {
  set t [expr {$rf + $offset}]
  wr [expr {$t + 0x04}] 1
  set up [wait_for [expr {$t + 0x0C}] 0xF 0xF 600]
  set state [rd [expr {$t + 0x0C}]]
  set status [rd [expr {$t + 0x228}]]
  check "$name: powered up" $up [format "(state %d, status 0x%08x)" [expr {$state & 0xF}] $status]
  check "$name: PLL locked" [expr {($status & 0x8) != 0}]
}

# ---- tones ----------------------------------------------------------------------------------------------------------
# 4096 words of 8 I/Q pairs = 32768 samples per loop at 2457.6 MS/s: `cycles` per loop is a tone of cycles * 75 kHz.
# I is the tone, Q zero; with the mixer's frequency at 0 the converter puts out I.
set pi 3.141592653589793
foreach {n cycles} {0 1365 1 2731} {
  set words {}
  for {set s 0} {$s < 32768} {incr s} {
    set i [expr {round(16384 * sin(2 * $pi * (($cycles * $s) % 32768) / 32768.0))}]
    lappend words [expr {$i & 0xFFFF}]
  }
  set mem $base(axi_bram_ctrl_$n)
  require_room axi_bram_ctrl_$n [expr {4 * [llength $words]}]
  write_words $mem $words
  check "player $n: waveform memory" [expr {[read_words [expr {$mem + 0x1FFC0}] 16] eq [lmap w [lrange $words end-15 end] {format 0x%08x $w}]}]
  set g $base(axi_gpio_play_$n)
  wr $g 0
  wr $g [expr {(4095 << 16) | 0x3}]
}
after 100

# ---- capture --------------------------------------------------------------------------------------------------------
set cg $base(axi_gpio_capture)
set dma $base(axi_dma_0)
set words 16384
set bytes [expr {$words * 16}]
# Capture from both inputs into memory and save each as <prefix>_<input>.hex for rf_analyse.py.
proc capture_both {prefix at_base what} {
  global cg dma words bytes work
  foreach {input name} {0 vin2_01 1 vin2_23} {
    set at [expr {$at_base + $input * 0x01000000}]
    set tag "$what, $name"
    write_words $at [lrepeat 16 0xDEADBEEF]
    wr [expr {$dma + 0x30}] 0x4
    check "$tag: DMA reset" [wait_for [expr {$dma + 0x30}] 0x4 0x0]
    wr [expr {$dma + 0x30}] 0x1001
    wr [expr {$dma + 0x48}] $at
    wr [expr {$dma + 0x4C}] 0
    wr [expr {$dma + 0x58}] $bytes
    set ctrl [expr {($words << 8) | ($input << 1)}]
    wr $cg $ctrl
    wr $cg [expr {$ctrl | 1}]
    set done [wait_for [expr {$dma + 0x34}] 0x1002 0x1002]
    check "$tag: capture reaches memory" $done [format "(DMA status 0x%08x, capture status 0x%x)" [rd [expr {$dma + 0x34}]] [rd [expr {$cg + 8}]]]
    expect "$tag: capture done, nothing dropped" [expr {[rd [expr {$cg + 8}]] & 0x7}] 0x2
    expect "$tag: bytes" [rd [expr {$dma + 0x58}]] $bytes
    check "$tag: DMA interrupt" [irq_line [irq0 0]]
    wr [expr {$dma + 0x34}] 0x7000
    if {$done} {
      set f [open $work/${prefix}_$name.hex w]
      puts $f [join [mrd -force -value $at [expr {$bytes / 4}]] \n]
      close $f
    }
  }
}
capture_both memory 0x20000000 "waveform memory"

# ---- constant mode: the player repeats one I/Q value and the converter's mixer makes the tone --------------------
# The mixer frequency is a 48-bit fraction of the 4915.2 MS/s sampling rate (registers as in the xrfdc driver):
# fs/16 = 307.2 MHz on DAC tile 0, fs/8 = 614.4 MHz on tile 2, applied at once.
foreach {n tile upper} {0 0 0x1000 1 2 0x2000} {
  set g $base(axi_gpio_play_$n)
  wr $g 0
  wr [expr {$g + 8}] 0x00004000
  wr $g 0x1
  set block [expr {$rf + 0x6000 + $tile * 0x4000}]
  wr [expr {$block + 0x94}] $upper
  wr [expr {$block + 0x98}] 0
  wr [expr {$block + 0x9C}] 0
  wr [expr {$block + 0x8C}] [expr {[rd [expr {$block + 0x8C}]] & ~0x7}]
  wr [expr {$block + 0x20}] [expr {([rd [expr {$block + 0x20}]] & ~0xF) | 0x2}]
  expect "DAC tile $tile: mixer frequency word" [rd [expr {$block + 0x94}]] $upper
}
after 100
capture_both mixer 0x24000000 "mixer tone"
