# ADR 0002: Bitstream embedding and incremental updates

Status: accepted

## Decision

Embed the bitstream in the generated crate (`include_bytes!`) as a raw `.bin`,
and keep two fingerprints so bitstream-only changes do not force regeneration.

## Motivation

The user wanted an embedded bitstream but also fast patching for minor changes
instead of a full regenerate on every update.

## Design

- The generated Rust code depends only on the design interface: IP instances,
  VLNVs, base addresses and ranges, register maps, interrupt lines. The
  bitstream is the payload implementing it. RTL fixes, HLS function-body
  changes, and timing/placement tweaks change the payload but not the
  interface.
- **Interface hash**: computed from Hardroxide's normalized HWH model (not raw
  XML, so Vivado upgrades do not cause spurious regeneration). Decides whether
  generated code is still valid.
- **Bitstream hash**: integrity check at load time.
- `hardroxide update <new.xsa>`: same interface hash means replace only the
  bitstream file and its hash (no Rust source change; cargo recompiles only the
  embedding crate). Different hash prints a structured diff (IP added/removed,
  base address moved, register changed, new IRQ) and requires `--regen`.
- Feature `dev-bitstream`: `Design::load_with_bitstream(path)` loads a new
  bitstream on the board without recompiling, provided the accompanying
  XSA/HWH interface hash matches the compiled one.
- Store `.bin`, not `.bit`: the `.bit` header contains timestamps, so identical
  builds would differ. Optional zstd compression to cut size and diff noise.
- FCLK frequencies are tentatively load config stored beside the bitstream, not
  part of the interface hash.

## Rejected

- Binary/frame-level bitstream patching: the format is only partially
  documented and the gain is small once interface and payload are separated.
- Partial reconfiguration is the real mechanism for swapping logic on a running
  device; deferred past v1.
