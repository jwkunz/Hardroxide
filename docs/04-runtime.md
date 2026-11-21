# 04 - Runtime

Status: draft, updated through interview round 3. This page is the Linux runtime; the bare-metal and RTOS runtimes are in [ADR 0003](decisions/0003-bare-metal-and-rtos-platforms.md), [`hardroxide-ps7`](reference/ps7.md) and [`hardroxide-rt`](reference/rt.md).

## Decisions

| Topic | Decision |
|---|---|
| Target OS (first) | Stock PYNQ-Z2 Ubuntu-based image. Document the minimum kernel features needed (fpga_manager, UIO, udmabuf/dma-heap) so other images can follow. |
| MMIO | Pluggable backend trait (map, read32, write32 and related). Backends: `/dev/mem` (default on PYNQ), UIO (required for interrupts), and a mock backend for tests. The bare-metal backend (`physmem`) shipped in 2.0. |
| DMA memory | `DmaBuffer` trait. Default backend: uncached, physically contiguous buffers (udmabuf / dma-heap), so no explicit cache maintenance in v1. A cached + explicit-sync backend is an opt-in performance feature added later (bare metal has the cache-maintenance helpers in `hardroxide-ps7`, but the DMA buffers it generates stay non-cacheable). ACP/HP coherency is not assumed. |

## Rationale

Open-source, general-AMD scope favors portability and correct-by-default
behavior over peak throughput. Register access costs the same under every
mapping method, so the MMIO choice is about permissions and portability, not
speed. DMA is where performance diverges, so it gets a trait with a safe default.

## Tradeoff summary

- `/dev/mem`: root required, whole physical memory exposed, works everywhere.
- UIO: per-device mapping and permissions, needs a device-tree node per IP, and
  is the standard route for PL interrupts.
- Uncached DMA: slow CPU access to the buffer, but no cache-coherency bugs.
- Cached DMA: fast CPU access, but a missing sync silently corrupts data;
  cache maintenance differs between ARMv7 and ARMv8.

## Decisions (round 3, continued)

| Topic | Decision |
|---|---|
| Testing | Golden XSA fixtures committed to the repo, snapshot tests of generated code, and driver unit tests against the mock MMIO backend. Hardware tests are separate and run manually on the PYNQ-Z2. |
| Partial reconfiguration | Out of scope for v1 (full-PL bitstreams only). The loader API must not preclude adding PR later. |
| Python interop | None. Rust only, clean break. |
| PL clocks / resets | Read FCLK frequencies and resets from the PS configuration in the HWH/XSA, with an optional user override. |

## Open questions

- Handling of PS peripherals (MIO UART/GPIO etc.) versus PL-only scope.
