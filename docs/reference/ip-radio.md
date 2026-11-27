# RFSoC base overlay radio blocks

`hardroxide_ip::radio` — the two small blocks of the radio in RFSoC-PYNQ's base overlays, new in 3.0:
`xilinx.com:ip:amplitude_controller` in front of a transmit converter and `xilinx.com:ip:packet_generator` after a
receiver. With the [converter driver](ip-rfdc.md) they send a tone and record it.

> **Verified on an RFSoC 4x2**, bare metal, with `apps/rfsoc4x2/baremetal/radio_loopback.rs` (2026-11-26); not yet
> under Linux. The registers are as RFSoC-PYNQ's drivers use them.

```rust,ignore
pub const SAMPLES_PER_WORD: usize = 8;
pub enum RadioError
pub struct AmplitudeController<R>
pub const FULL_SCALE: u16 = 0x7FFF;
pub fn new(regs: R) -> Self
pub fn regs(&self) -> &R
pub fn set_amplitude(&mut self, amplitude: u16) -> Result<(), RadioError>
pub fn amplitude(&self) -> Result<u16, MmioError>
pub fn enable(&mut self) -> Result<(), MmioError>
pub fn disable(&mut self) -> Result<(), MmioError>
pub fn is_enabled(&self) -> Result<bool, MmioError>
pub struct PacketGenerator<R>
pub fn set_length(&mut self, words: usize) -> Result<(), RadioError>
pub fn length(&self) -> Result<usize, MmioError>
pub fn send(&mut self) -> Result<(), MmioError>
pub fn stop(&mut self) -> Result<(), MmioError>
pub fn record<R: Mmio, B: DmaBuffer>(
```

> **Warning.** These blocks and the receivers' DMAs run on the converters' clocks. **Until the board's clock chips are
> programmed they do not answer, and an access can stall the processor until the next reset.** Program the clocks, then
> [start the tiles](ip-rfdc.md) (which fails with an error, safely, if there is no clock), and only then touch a block
> here. Creating a driver (`new`, and so a generated `Design`) accesses nothing.

## Transmit

The block repeats one value into the converter's stream, on both I and Q; it has no waveform memory. **The tone is made
by the DAC's mixer**: set its frequency with [`Rfdc::set_mixer`](ip-rfdc.md), then `set_amplitude` and `enable`.

## Receive

Each receiver gives an I stream and a Q stream, each to its own DMA, and the packet generator passes a set number of
words of both on request. `record(generator, i_dma, q_dma, buffers, samples, timeout)` does it in the order that works:
it starts both DMAs, then asks for the packet, then waits for both.

- `samples`: 16 to 32768 in steps of 8, signed 16-bit little-endian, I in the first buffer and Q in the second, each
  pair taken at the same instant. The buffers need `2 * samples` bytes.
- On an error the DMAs are stopped and the buffers are not given back. A record that does not arrive whole is an error
  (`Short`, or the DMA's timeout), and whatever it left on the way is discarded by the next `record` before it asks for
  a packet.
- **RFSoC 4x2 base overlay: record at most 30720 samples from `radio_receiver_channel_20` (ADC B).** Its path to memory
  has no data buffer in RFSoC-PYNQ's bitstream, and longer records lost their end every time; the other three receivers
  take 32768. Measured with the processor otherwise idle, so leave a margin if it is busy
  ([results](../decisions/0006-radio.md#steps)).
- Receivers are recorded one at a time; nothing ties one receiver's start to another's or to a transmitter's.

Which socket each receiver of the RFSoC 4x2's base overlay is: `radio_receiver_channel_21` is ADC A and
`radio_receiver_channel_20` ADC B, as the ports show; ADC C and D have not yet been matched to channels 00 and 01.
`radio_transmitter_channel_20` is DAC A and `radio_transmitter_channel_00` DAC B.
