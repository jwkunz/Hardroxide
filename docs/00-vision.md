# 00 - Vision

## Summary

Hardroxide is a code generator and runtime library that turns a Vivado XSA
(bitstream + hardware handoff) into a typed Rust HAL crate. It is a Rust
counterpart to the PYNQ overlay framework.

> **Updated for 2.0.** The "Runtime" row below is the 1.0 decision. 2.0 added two more flavors of generated
> crate (bare metal and an RTOS, `--platform baremetal|rtos`); see
> [ADR 0003](decisions/0003-bare-metal-and-rtos-platforms.md).

## Decisions from interview (round 1)

| Topic | Decision |
|---|---|
| Runtime | Linux on the Processing System (userspace Rust binary). Bare-metal `no_std` was a possible later backend, so the generator stayed backend-agnostic: it shipped in 2.0 (bare metal and a real-time kernel). |
| User experience | Both equally: library-first for embedded app development, with a thin REPL/CLI on top for PYNQ-style exploration. |
| Register source | HWH data where present, plus a curated IP driver library keyed by VLNV. Unknown IP falls back to raw register access. |
| Scope | Open source, aimed at general AMD devices (Zynq-7000 and UltraScale+), not just one board. |

## Starting point

- First board: PYNQ-Z2 (XC7Z020, dual Cortex-A9, armv7 hard-float)
- Vivado 2025.2
- Input format: `.xsa`

## Goals

- Generate a readable, versionable, publishable Rust crate from an XSA.
- Compile-time typed access to every IP instance in the design.
- Safe-by-construction DMA buffers and memory-mapped register access.
- Architecture that does not hard-code PYNQ-Z2 assumptions (clocks, PS config,
  address maps come from the XSA/board profile).

## Non-goals (tentative)

- Replacing Vivado or synthesizing hardware.
- A Jupyter-first experience.

## Open questions

- Board profile abstraction: how board-specific data is separated from the generator.
- Crate naming scheme beyond the `hardroxide-*` prefix.
