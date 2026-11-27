# Hardroxide Design Documentation

Hardroxide generates a Rust hardware abstraction layer (HAL) crate from a Vivado
XSA export, as a Rust alternative to the PYNQ overlay framework.

## Status

The 00-06 documents record the design interviews for the first (Linux) release and are kept as written, with notes
where 2.0 changed them. The bare-metal and RTOS platforms are specified in [ADR 0003](decisions/0003-bare-metal-and-rtos-platforms.md). What is built
and verified is in the [reference](reference/cli.md) and [Verification gaps](08-verification-gaps.md).

## Documents

| Doc | Purpose |
|---|---|
| [00-vision.md](00-vision.md) | Goals, non-goals, target users |
| [01-requirements.md](01-requirements.md) | Functional and non-functional requirements |
| [02-architecture.md](02-architecture.md) | Crates, pipeline, layers |
| [03-generated-crate.md](03-generated-crate.md) | Shape of the generated HAL crate |
| [04-runtime.md](04-runtime.md) | Target runtime: Linux on PS, MMIO, DMA, IRQ (bare metal and RTOS: ADR 0003) |
| [05-roadmap.md](05-roadmap.md) | Policies and milestones (Milestone 1: GPIO end-to-end) |
| [06-session-handoff.md](06-session-handoff.md) | Context from the original design session, risks, first steps |
| [07-hwh-findings.md](07-hwh-findings.md) | What a real Vivado 2025.2 HWH contains |
| [release.md](release.md) | Maintainer checklist for a release |
| [08-verification-gaps.md](08-verification-gaps.md) | What is unverified without hardware, and how to check each item |
| [decisions/](decisions/) | ADRs: 0001 MMIO/DMA model, 0002 bitstream embedding and updates, 0003 bare-metal and RTOS platforms, 0004 base overlay applications (2.1 to 2.7, accepted), 0005 Zynq UltraScale+ (3.0, accepted), 0006 radio driver (3.1, accepted) |

## Baseline context

- First target board: PYNQ-Z2 (Zynq-7000, XC7Z020, ARMv7)
- Toolchain: Vivado 2025.2
- Input: `.xsa` (contains `.bit` and `.hwh`)
