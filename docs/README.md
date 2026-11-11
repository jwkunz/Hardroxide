# Hardroxide Design Documentation

Hardroxide generates a Rust hardware abstraction layer (HAL) crate from a Vivado
XSA export, as a Rust alternative to the PYNQ overlay framework.

## Status

Specification phase. Specs are filled in through interviews; open questions are
tracked in each document.

## Documents

| Doc | Purpose |
|---|---|
| [00-vision.md](00-vision.md) | Goals, non-goals, target users |
| [01-requirements.md](01-requirements.md) | Functional and non-functional requirements |
| [02-architecture.md](02-architecture.md) | Crates, pipeline, layers |
| [03-generated-crate.md](03-generated-crate.md) | Shape of the generated HAL crate |
| [04-runtime.md](04-runtime.md) | Target runtime: Linux on PS, MMIO, DMA, IRQ |
| [05-roadmap.md](05-roadmap.md) | Policies and milestones (Milestone 1: GPIO end-to-end) |
| [06-session-handoff.md](06-session-handoff.md) | Context from the original design session, risks, first steps |
| [07-hwh-findings.md](07-hwh-findings.md) | What a real Vivado 2025.2 HWH contains |
| [08-verification-gaps.md](08-verification-gaps.md) | What is unverified without hardware, and how to check each item |
| [decisions/](decisions/) | ADRs: 0001 MMIO/DMA model, 0002 bitstream embedding and updates |

## Baseline context

- First target board: PYNQ-Z2 (Zynq-7000, XC7Z020, ARMv7)
- Toolchain: Vivado 2025.2
- Input: `.xsa` (contains `.bit` and `.hwh`)
