# Running on hardware

No driver has been validated on a board yet. Before relying on anything,
work through the checklist in [Verification gaps](../08-verification-gaps.md)
**in order**:

1. **G1**: the kernel accepts the embedded `.bin` and programs the PL. Do not
   read PL addresses before this passes: an AXI access to unprogrammed PL
   usually hangs the CPU.
2. **G2/G3**: PL clocks and resets.
3. **G4/G5**: `/dev/mem` mapping and the GPIO smoke test.
4. **G9–G12**: HLS, DMA, UIO interrupts, Timer/IIC/SPI.

Every generated crate has a first-contact example, `board_check`, that covers
G1, G2, G4 and the interrupt-device part of G11 in one run and touches no IP
register:

```console
$ sudo -E env "PATH=$PATH" cargo run --example board_check
```

It prints one line per step and exits non-zero if any step fails. Run it first,
then the per-IP examples below.

Each generated crate carries ready-made smoke tests under `examples/`
(`gpio_smoke`, `hls_smoke`, `dma_loopback`, `timer_delay`, `spi_loopback`,
`i2c_scan`), present when the design contains the matching IP:

```console
$ sudo -E env "PATH=$PATH" cargo run --example gpio_smoke
```
