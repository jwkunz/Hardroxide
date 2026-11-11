# ADR 0001: MMIO backends and DMA memory model

Status: accepted

## Decision

- MMIO is a pluggable backend trait. `/dev/mem` is the default on PYNQ, UIO is
  used for interrupts, and a mock backend supports hardware-free tests.
- DMA buffers sit behind a `DmaBuffer` trait. The default backend is uncached,
  physically contiguous memory (udmabuf / dma-heap). Cached + explicit sync is
  a later opt-in. ACP/HP coherency is not assumed.

## Context and tradeoffs considered

### Register access

| | `/dev/mem` | UIO | Pluggable |
|---|---|---|---|
| Performance | Identical once mmap'd (direct load/store) | Identical | Identical (mock is test-only) |
| Security | Weakest: root, all physical memory exposed; may be blocked by `CONFIG_STRICT_DEVMEM` | Per-device mapping and permissions, no root needed | Inherits backend |
| Portability | Works anywhere with no device-tree change; unavailable on hardened kernels and bare-metal | Needs a device-tree node per IP (`generic-uio`); on stock PYNQ that means a DT overlay per design | Best: same driver code on hardware, mock, later bare-metal |
| Interrupts | Not supported | Standard Linux route for PL IRQs | Per backend |

Interrupts force UIO to exist regardless, so `/dev/mem` alone is not enough.
The trait costs about three methods and buys the mock backend.

### DMA buffers

| | Uncached contiguous | Cached + explicit sync | ACP/HP coherent |
|---|---|---|---|
| CPU speed | Slowest (every access hits DRAM) | Fastest | Fast |
| Correctness risk | Lowest | Highest: a missed sync silently yields stale data | Low only if the design really uses ACP |
| Portability | Needs udmabuf or dma-heap (present on PYNQ) | Cache maintenance differs between ARMv7 (Zynq-7000) and ARMv8 (UltraScale+) | Cannot be inferred from the XSA alone |

Rationale: open-source, general-AMD scope favors portability and
correct-by-default over peak throughput. A safe `DmaBuffer` that enforces sync
at ownership handoff could later make the cached backend safe.
