# RFSoC 3: The board's own hardware

`baremetal/rfsoc-board` uses everything on the board that the processor reaches directly:
[`hardroxide-board`](../../reference/board.md) for the chips, and the SPI, I2C, GPIO and temperature drivers of
[`hardroxide-psu`](../../reference/psu.md).

```console
$ cd baremetal/rfsoc-board
$ cargo build --release
$ hardroxide run target/aarch64-unknown-none/release/rfsoc-board \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 60
```

```text
== RFSoC 4x2 board demo ==
identity (AT24MAC402 on I2C 1):
  Ethernet address xx:xx:xx:xx:xx:xx
  serial number (32 hex digits)
die temperatures:
  processing system: 41.6 C
  programmable logic: no reading (the FPGA design's own monitor has the sensor)
power monitors (INA220 on I2C 0):
  VCC0V85         884 mV   6710 mA   5931 mW
  VCC3V3         3340 mV    380 mA   1269 mW
  VCC1V8         1828 mV   1100 mA   2010 mW
  VDAC_AVCC       956 mV    352 mA    336 mW
  VADC_AVCC       960 mV    965 mA    926 mW
  VADC_AVCCAUX   1824 mV    570 mA   1039 mW
  VDAC_AVCCAUX   1832 mV    117 mA    214 mW
  VDAC_AVTT      2532 mV     93 mA    235 mW
  SYZYGY_VIO     3400 mV    147 mA    499 mW
  12459 mW through the nine monitored rails
OLED display:
  SPI 1 at 732421 Hz
  address counter read back: Some(10), Some(50), Some(25)
clock chips:
  SPI 0 at 732421 Hz
  with the LMK04828 reset to its power-on state:
  LMX2594 (DAC): UnlockedHigh
  LMX2594 (ADC): Locked
  LMK04828 programmed (136 writes):
  LMX2594 (DAC): Locked
  LMX2594 (ADC): Locked
processor LEDs and button:
  button: released
BOARD DEMO OK
press PS_BTN to step through the display's pages
page 0: [RFSoC 4x2       ] [MAC xxxxxxxxxxxx]
```

The program keeps running. The display shows the first page; each press of `PS_BTN` goes to the next (the die
temperatures, then each supply, updated twice a second). One processor LED blinks and the other follows the button.

## Reading the results

- **Power** depends on the FPGA design: about 12.5 W on these rails with the base overlay, about 5 W with a small test
  design.
- **Nothing permanent is written.** The identity EEPROM and the power monitors are only read. The clock chips are
  programmed with the files in `fixtures/rfsoc4x2/clocks/` and forget at power-off.
- **The display** cannot be seen by the program; it reads the controller's address counter back to know the display is
  listening.
- **The clock chips:** the program first resets the main chip, and the DAC synthesizer must then fail to lock, which
  shows the lock readout can say no. The ADC synthesizer reports lock even then, so its "Locked" alone does not prove
  the frequency. That was checked with the converters: after this setup their PLLs lock, and in a cable loopback test the
  tones arrive at the intended frequencies.
- **The FPGA's temperature** reads "no reading" with the base overlay, which has a monitor block of its own for that
  sensor ([tutorial 2](02-base-overlay.md) reads it there).
- **One pin to respect:** `MIO7` goes to the main clock chip's reset. Drive it low, never high
  ([why](../../reference/board.md#clock-chips-tics-lmk04828-lmx2594)).

Next: [the real-time kernel](04-kernel.md).
