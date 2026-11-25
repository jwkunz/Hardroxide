# hardroxide-board

Drivers for chips found on FPGA boards, new in 3.0. They use the `embedded-hal` 1.0 I2C and SPI traits, so they work with
`hardroxide-ps7`/`hardroxide-psu`'s controllers (`I2c` implements `I2c`; [`Spi::device`](ps7.md#spi-controllers) gives a
chip select as an `SpiDevice`) and with any other implementation. `no_std`, no allocation. The module `rfsoc4x2` says how
the chips are wired on the Real Digital RFSoC 4x2; `baremetal/rfsoc-board` uses all of them.

> **Verified on an RFSoC 4x2** with `baremetal/rfsoc-board` (2026-11-25): the identity EEPROM, all nine monitors, the
> display's address counter read back, both synthesizers' lock readout (including "unlocked" without a reference) and the
> clock programming, confirmed by the RF converters (their PLLs lock and loopback tones arrive at the exact frequencies:
> `HRX_RF_KEEP_CLOCKS=1 fixtures/rfsoc4x2/scripts/probe.py rf_loopback`). What the display shows and the button need a
> person to check. Not run on other boards.

## Power monitor (`ina220`)

```rust,ignore
pub const RESET_CONFIGURATION: u16 = 0x399F;
pub struct Reading
pub fn milliamps(&self, shunt_micro_ohms: u32) -> i32
pub fn milliwatts(&self, shunt_micro_ohms: u32) -> i32
pub fn configuration<I: I2c>(bus: &mut I, address: u8) -> Result<u16, I::Error>
pub fn read<I: I2c>(bus: &mut I, address: u8) -> Result<Reading, I::Error>
```

A TI INA220 as it comes out of reset: measuring continuously, bus voltage in 4 mV steps, shunt voltage in 10 uV steps up
to 320 mV. The current is computed from the shunt voltage and the resistor's value, so nothing is configured: the only
bytes written are register pointers.

## Identity EEPROM (`at24mac`)

```rust,ignore
pub const IDENTITY_ADDRESS: u8 = 0x58;
pub struct MacAddress(pub [u8; 6]);
pub fn is_unicast(&self) -> bool
pub fn mac_address<I: I2c>(bus: &mut I, address: u8) -> Result<MacAddress, I::Error>
pub fn serial_number<I: I2c>(bus: &mut I, address: u8) -> Result<[u8; 16], I::Error>
```

The factory-programmed Ethernet address and 128-bit serial number of a Microchip AT24MAC402. There is no way to write
with this module.

## OLED display (`us2066`)

```rust,ignore
pub const ROWS: usize = 2;
pub const COLUMNS: usize = 16;
pub struct Oled<S: SpiDevice>
pub fn new(spi: S) -> Self
pub fn init(&mut self) -> Result<(), S::Error>
pub fn clear(&mut self) -> Result<(), S::Error>
pub fn write_line(&mut self, row: usize, text: &str) -> Result<(), S::Error>
pub fn address(&mut self) -> Result<Option<u8>, S::Error>
pub fn set_address(&mut self, address: u8) -> Result<(), S::Error>
```

The US2066 controller of a NewHaven NHD-0216AW (16 characters, 2 rows) in SPI mode 3 at up to 1 MHz. Text is printable
ASCII in the controller's character set A; other characters show as `?`. `address` reads the controller's address
counter, which is the one way to tell from software that the display is there. The initialisation sequence is
RFSoC-PYNQ's.

## Clock chips (`tics`, `lmk04828`, `lmx2594`)

```rust,ignore
pub struct BadLine
pub fn registers(text: &str) -> impl Iterator<Item = Result<u32, BadLine>> + '_
pub fn parse<'a>(text: &str, buffer: &'a mut [u32]) -> Result<&'a [u32], BadLine>
pub enum Lock
pub enum Error<E>
pub fn reset<S: SpiDevice>(spi: &mut S) -> Result<(), S::Error>
pub fn write<S: SpiDevice>(spi: &mut S, word: u32) -> Result<(), S::Error>
pub fn program<S: SpiDevice>(spi: &mut S, words: &[u32]) -> Result<(), S::Error>
pub fn program<S: SpiDevice>(spi: &mut S, words: &[u32]) -> Result<(), Error<S::Error>>
pub fn read_register<S: SpiDevice>(spi: &mut S, register: u8) -> Result<u16, S::Error>
pub fn lock<S: SpiDevice>(spi: &mut S, words: &[u32]) -> Result<Lock, Error<S::Error>>
```

Both TI chips are programmed from the register files TICS Pro exports (those for the RFSoC 4x2 are in
`fixtures/rfsoc4x2/clocks/`). `lmx2594::lock` first reads registers back and compares them with the file, so a silent
chip is an error, not "unlocked" or "locked"; it uses the `MUXout` pin to read, and leaves it as the file set it.

**The LMK04828's reset pin** is an input only until a register file says otherwise. The RFSoC 4x2's makes it an
open-drain output held low, and the processor pin wired to it has a pull-up that holds the chip in reset if left floating.
So drive that pin (`rfsoc4x2::LMK_RESET`) low, never high, and reset the chip with `lmk04828::reset`.

## RFSoC 4x2 wiring (`rfsoc4x2`)

```rust,ignore
pub const PS_LEDS: [u32; 2] = [20, 17];
pub const PS_BUTTON: u32 = 23;
pub const MONITOR_I2C: usize = 0;
pub struct Rail
pub const IDENTITY_I2C: usize = 1;
pub const OLED_SPI: usize = 1;
pub const CLOCK_SPI: usize = 0;
pub const LMK_CHIP_SELECT: u8 = 0;
pub const LMX_DAC_CHIP_SELECT: u8 = 1;
pub const LMX_ADC_CHIP_SELECT: u8 = 2;
pub const SPI_MAX_HZ: u32 = 1_000_000;
pub const LMK_RESET: u32 = 7;
pub const LMK_CLOCK_SELECT: [u32; 2] = [8, 12];
```

`RAILS` lists the nine monitored supplies with their I2C addresses and shunt resistors. One observation from the board:
the ADCs' synthesizer reports lock even with the LMK04828 freshly reset, so a clock reaches it then; only the converters
show whether it is the right one.
