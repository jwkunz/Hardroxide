# 08 - Verification gaps (no hardware available)

Everything below was implemented without a PYNQ-Z2 attached. Each item says
what was assumed, what evidence exists, how to verify on the board, and what
to change if the assumption is wrong. Work through the list in order: later
items depend on earlier ones. When an item is verified, change its status here
and record the date and image/kernel version.

Status key: **VERIFIED-OFFLINE** (checked against a vendor tool, no board
needed), **UNVERIFIED** (needs the board), **DESIGN-GAP** (known incomplete).

Prerequisites on the board: stock PYNQ-Z2 image, root shell, Rust toolchain
or a cross-compile (`rustup target add armv7-unknown-linux-gnueabihf`; the
workspace is known to `cargo check` for that target, but no linker/sysroot
has been set up). Record `uname -a` and the PYNQ image version here.

## Verified offline

| ID | Item | Evidence |
|---|---|---|
| V1 | `.bit` -> `.bin` conversion (header strip, per-word byte swap, NOP padding to 16 bytes) | Output of `hardroxide generate` is byte-identical to `bootgen -arch zynq -process_bitstream bin` (Vivado 2025.2) for `fixtures/pynq_z2_gpio.xsa`. Reproduce: bif `all:{pynq_z2_gpio.bit}`, `bootgen -image a.bif -arch zynq -process_bitstream bin -w on`, `cmp` against `bitstream/design.bin`. Whether the *kernel* accepts this format is G1. |
| V2 | HWH parsing (instances, registers, fields, address map, FCLK) | Single fixture, 2025.2 only (see 07). Not yet tested on HLS/DMA/other Vivado versions. |
| V3 | Generated crate compiles warning-free; mock-MMIO behaviour; singleton semantics; load-failure releases the singleton | `cargo test -p hardroxide-gen` generates, builds and tests a real crate. |
| V4 | `/dev/mem`-style mmap path (page alignment, offsets, volatile access) | Unit test maps a regular file at an unaligned offset. Does not exercise real `/dev/mem`. |
| V5 | Workspace type-checks for `armv7-unknown-linux-gnueabihf` | `cargo check --workspace --target armv7-unknown-linux-gnueabihf`. |

## Needs the board

### G1. fpga_manager accepts the `.bin` and programs the PL (UNVERIFIED)
- Assumed: write `0` to `/sys/class/fpga_manager/fpga0/flags`, copy bin to
  `/lib/firmware/<name>`, write `<name>` to `.../fpga0/firmware`, then
  `.../state` reads `operating` (`hardroxide-fpga::load_bitstream`).
- Check: `cat /sys/class/fpga_manager/fpga0/{name,state,flags}`; manually
  `cp bitstream/design.bin /lib/firmware/t.bin; echo t.bin > .../firmware`;
  `dmesg | tail`. Then run `cargo run --example gpio_smoke`.
- If wrong: the device name may differ (`fpga0`), `state` may report
  differently (e.g. `write complete`), the `flags` attribute may be absent on
  this kernel (then skip it), or the image may need the *unswapped* bin / the
  legacy `/dev/xdevcfg` path (PYNQ's older images). Fix in `hardroxide-fpga`;
  if unswapped is needed, change `BitFile::to_bin_swapped` and V1.
- Also: the firmware file is written to `/lib/firmware` and deleted after
  load; confirm the firmware path search works when `firmware_dir` is the
  default, and whether the kernel needs `firmware_class.path`.

### G2. PL clock programming via SLCR (UNVERIFIED)
- Assumed (UG585): SLCR at `0xF8000000`; unlock `0x008`<-`0xDF0D`, lock
  `0x004`<-`0x767B`; `ARM/DDR/IO_PLL_CTRL` at `0x100/0x104/0x108` with
  `PLL_FDIV` in bits 18:12 and `PLL_BYPASS_FORCE` bit 4; `FPGAn_CLK_CTRL` at
  `0x170/0x180/0x190/0x1A0` with `SRCSEL` bits 5:4 and `DIVISOR0/1` in bits
  13:8 / 25:20; PS_CLK from the HWH (`hardroxide-fpga::slcr`).
- Check: before running, read `devmem 0xF8000170`, `0xF8000108`; run the
  program; confirm FCLK0 = 100 MHz (e.g. `cat /sys/kernel/debug/clk/...` if
  available, or measure an AXI-clocked counter, or compare with PYNQ's
  `Clocks.fclk0_mhz` if Python is available).
- Risks: SRCSEL encoding (0/1 = IO PLL, 2 = ARM PLL, 3 = DDR PLL). The PS
  reference clock is taken from the HWH (`PCW_CRYSTAL_PERIPHERAL_FREQMHZ`,
  emitted as `PS_CLK_HZ`); PYNQ's own base design uses 50 MHz, and the fixture
  was rebuilt with 50 so it matches the board. Confirm the real board crystal.
  Writing SLCR through `/dev/mem` may conflict with a kernel clock driver that
  caches the rate; check `dmesg` and that other peripherals are unaffected.
- Also check whether `fpga_manager` load resets/gates the FCLKs, which would
  require applying clocks *after* load (current order) or re-enabling them.

### G3. PL reset (FCLK_RESET) handling (DESIGN-GAP)
- Not implemented. The HWH exposes `PCW_EN_RSTn_PORT` (modeled as
  `PlClock::reset_enabled`) but `load()` does not touch `FPGA_RST_CTRL`
  (SLCR `0x240`). Check whether the fabric is in reset after load; if so,
  deassert via SLCR (unlock, clear bits, lock) and add to `hardroxide-fpga`.
  Also AXI level shifters (`LVL_SHFTR_EN`, SLCR `0x900`) may need enabling.

### G4. `/dev/mem` mapping of PL addresses (UNVERIFIED)
- Assumed: `open("/dev/mem", O_RDWR|O_SYNC)` + `mmap` at `0x41200000` works
  as root (no `CONFIG_STRICT_DEVMEM` block for non-RAM ranges).
- Check: `devmem 0x41200000` before and after load (before load an AXI access
  to unprogrammed PL typically hangs or bus-errors the CPU: **do not read PL
  addresses before G1 passes**).
- If wrong: use the UIO backend (needs a device-tree overlay per IP).
- mmap uses `O_SYNC`; confirm the mapping is device/strongly-ordered memory
  (not cached) so volatile accesses reach the AXI bus in order.

### G5. AXI GPIO driver semantics and the example (UNVERIFIED)
- `hardroxide-ip::gpio` follows PG144 offsets (DATA 0x0, TRI 0x4, DATA2 0x8,
  TRI2 0xC), `1` = input in TRI. HWH offsets agree (fixture test), but real
  pin behaviour is unverified.
- Check: `cargo run --example gpio_smoke` in a generated crate: 4 LEDs walk a
  one-hot pattern; press buttons/switches and confirm channel 2 reads
  `0b[sw1 sw0 btn3 btn2 btn1 btn0]` as wired by the fixture's `pins.xdc`.
- Fixture pin map (from `fixtures/scripts/build_pynq_z2_gpio.tcl`): LEDs
  R14 P14 N16 M14; buttons D19 D20 L20 L19; switches M20 M19. Verify against
  the PYNQ-Z2 schematic/master XDC; the fixture was built without a board file.

### G6. Fixture fidelity (UNVERIFIED)
- The fixture PS7 is hand-configured (no PYNQ-Z2 board preset): DDR part,
  MIO and peripherals are not board-accurate (the crystal is set to 50 MHz). Since
  loading via Linux only reprograms the PL this should be harmless, but if
  `ps7_init` side effects or the crystal value matter (see G2), rebuild the
  fixture with the real PYNQ-Z2 board files / preset (`tul.com.tw:pynq-z2`)
  and refresh the golden files (`UPDATE_GOLDEN=1 cargo test -p hardroxide-gen`).

### G7. Singleton and process behaviour on the board (UNVERIFIED)
- The singleton is an in-process `AtomicBool`; it does not stop a second
  *process* (or PYNQ's Python) from loading another bitstream underneath a
  running program. Decide whether to add a lock file (`/run/hardroxide.lock`).
- Check what happens to a running design when the process exits (PL keeps
  running; registers keep state). Document the intended semantic.

### G8. Cross-compilation and deployment (UNVERIFIED)
- Only `cargo check` for armv7 was run. Set up a linker
  (`arm-linux-gnueabihf-gcc` or `cargo-zigbuild`/`cross`) and confirm a
  release build runs on the stock image's glibc. The embedded bitstream makes
  binaries ~4 MB.

### G9. HLS IP on hardware (UNVERIFIED)
- `hardroxide-ip::hls::HlsIp` follows the `ap_ctrl_hs` layout from the
  generated RTL header (CTRL/GIER/IP_IER/IP_ISR, user regs from 0x10).
  `start()` reads CTRL first (to preserve `auto_restart`), which clears the
  read-to-clear `ap_done`/`ap_ready` bits as a side effect, as in Xilinx's own
  driver.
- Check with the vadd fixture: load it, `cargo run --example hls_smoke`
  (reads status only), then in a scratch program `set_a(40)`, `set_b(2)`,
  `start()`, `wait_done(..)`, `out_r() == 42`, also `ap_ready`/`ap_idle`
  transitions and `out_r_ctrl` bit 0 (`ap_vld`).
- Interrupt wiring (`PS7 IRQ_F2P[0]`) is only recorded in the PAC
  (`INTERRUPTS`); no interrupt delivery exists yet (D1).

### G10. AXI DMA direct mode + udmabuf on hardware (UNVERIFIED)
- Driver follows PG021 simple mode; register offsets/bits are asserted
  against the HWH (offline), behaviour is not. Mock models write-1-to-clear
  DMASR flags, which is an assumption about the real register semantics.
- Check: load `pynq_z2_dma_loopback`, `insmod u-dma-buf` twice
  (`udmabuf0`, `udmabuf1`, >= 4 KiB each; confirm the module exists on the
  PYNQ image, otherwise build it or use CMA/dma-heap), then `cargo run
  --example dma_loopback`. Expect "DMA loopback OK".
- Things that could differ: udmabuf sysfs paths (`/sys/class/u-dma-buf/...`),
  `O_SYNC` meaning uncached for that module version, buffer physical address
  reachable by HP0 (< 512 MB DDR), S2MM needing TLAST/length semantics (the
  FIFO loopback relies on MM2S asserting TLAST), the S2MM-before-MM2S
  ordering, and `reset()` timing. Also whether the AXI level shifters /
  HP port need enabling (G3).
- `Transfer` halts a channel on timeout/error/drop and leaks the buffer if the
  channel will not halt; confirm the halted state is reached on real hardware.

### G11. UIO interrupts (UNVERIFIED)
- `hardroxide-irq::UioIrq` implements the standard UIO protocol and is
  tested only against a socketpair. On the stock PYNQ image PL interrupts need
  a `generic-uio` device-tree node (overlay) per IP; none is generated yet.
- The generated PAC reports the PS7 `IRQ_F2P` bit per interrupt. Mapping the
  bit to a GIC SPI (Zynq-7000: bits 0-7 -> SPI 61-68, bits 8-15 -> SPI 84-91;
  UG585 table 7-4, unverified here) and to the Linux IRQ number is not
  implemented.
- Check: add a DT overlay exposing `axi_dma_0` (or `vadd_0`) as `uio` with the
  matching `interrupts = <0 29 4>` (SPI 61 -> GIC `29`), then wait on it while
  triggering a transfer and verify `wait()` returns, `ack` works, and the line
  is re-armed.

## Design gaps (implemented partially or not at all)

- **D1 Interrupts**: ports, sinks and PS7 `IRQ_F2P` bits (direct or through
  `xlconcat`) are parsed and exposed in the PAC; `hardroxide-irq` provides UIO
  blocking and feature-gated async waiting. Not done: GIC/Linux IRQ number
  mapping, DT overlay generation, wiring `UioIrq` into generated `Design`,
  routes through logic other than `xlconcat`/direct.
- **D2 Other IP**: AXI GPIO is curated and HLS IP gets generated typed
  accessors (scalar 32-bit registers only: no memory-mapped arrays, no 64-bit
  pointer register pairs helper, no `ap_ctrl_chain` specifics, no m_axi
  buffers). Custom RTL without register data gets a raw window. AXI DMA has a direct-mode driver (no SG, no 64-bit addressing check, no
  mixed MM2S/S2MM-only configuration handling). Timer/IIC/SPI need their own
  fixtures (docs/05 milestone 5). Unknown IP is exposed
  as a raw MMIO window.
- **D3 Multiple address blocks / segments**: the PAC emits only the first
  address range of an instance and the registers of its matching block.
- **D4 `hardroxide update` / `dev-bitstream`** (milestone 2): implemented and
  tested offline (bitstream-only update touches exactly `bitstream/design.bin`
  and `design.sha256`; interface change prints a line diff and needs
  `--regen`; `Design::load_with_bitstream` checks the XSA interface hash).
  Gaps: the diff is a line-set diff of the canonical interface text, not
  grouped by IP; `--regen` keeps the existing `Cargo.toml` and does not
  accept a new dependency path; the interface hash is compared only at update
  and `load_with_bitstream`, never against the running hardware. Not exercised
  on the board (G1 prerequisite).
- **D5 Other Vivado versions / designs**: HWH parsing is validated against one
  2025.2 file. Collect more samples (older versions, HLS, DMA, UltraScale+).
- **D6 `cargo hardroxide` shim, optional zstd, rename map, async**: not done.
- **D7 UIO backend**: `Uio` maps map 0 only; untested against a real device.
- **D8 No CI** configuration yet; `cargo test --workspace` is hardware-free.
