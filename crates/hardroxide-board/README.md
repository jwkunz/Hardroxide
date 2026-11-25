# hardroxide-board

Drivers for chips found on FPGA boards, written against the `embedded-hal` I2C and SPI traits, so they run wherever
those do (`no_std`, no allocation):

| Module | Chip |
|---|---|
| `ina220` | TI INA220 power monitor |
| `at24mac` | Microchip AT24MAC402 EEPROM: the board's Ethernet address and serial number (read only) |
| `us2066` | the OLED controller of the NewHaven NHD-0216AW 16x2 display |
| `lmk04828`, `lmx2594` | TI clock jitter cleaner and RF synthesizer, programmed from TICS Pro register files |
| `rfsoc4x2` | how those are wired on the Real Digital RFSoC 4x2 |

Part of [Hardroxide](https://github.com/jwkunz/Hardroxide). License `MIT OR Apache-2.0`.
