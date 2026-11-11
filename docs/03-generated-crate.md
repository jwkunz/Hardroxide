# 03 - Generated Crate

Status: draft, updated through interview round 5.

## Bitstream strategy

| Topic | Decision |
|---|---|
| Bundling | Embedded in the generated crate with `include_bytes!`. |
| Stored format | Raw `.bin` (not `.bit`), because the `.bit` header carries timestamps that make identical builds differ. Optional compression (e.g. zstd) to cut size and diff noise. |
| Fingerprints | Two hashes: an **interface hash** (normalized HWH model: IP instances, VLNVs, base addresses and ranges, register maps, interrupts) and a **bitstream hash** (integrity check at load). |
| Incremental update | `hardroxide update <new.xsa>`: if the interface hash matches, replace only the bitstream file and its hash, with no Rust source changes. If it differs, print a structured diff and require `--regen` to regenerate. |
| Dev iteration | Feature `dev-bitstream` adds `Design::load_with_bitstream(path)`, which loads a bitstream from a file at runtime when the accompanying XSA/HWH interface hash matches the compiled one. Release builds are embedded-only. |
| Not pursued | Binary/frame-level bitstream patching. Partial reconfiguration remains a later feature. |

## Design details

- The interface hash covers Hardroxide's normalized model, not raw XML, so a
  Vivado upgrade does not cause spurious regeneration.
- FCLK frequencies are treated as load config stored beside the bitstream, not
  as part of the interface hash (tentative).
- Regeneration is deterministic: an unchanged interface yields no source churn.

## Ownership, naming, errors

| Topic | Decision |
|---|---|
| Ownership | `Design::load()` returns a struct that owns each IP driver. Only one instance may exist per process, so two handles cannot alias the same registers. |
| Naming | Field names are the snake_case of the Vivado instance name (`axi_gpio_0` stays `axi_gpio_0`). Invalid characters are sanitized and collisions resolved deterministically. An optional TOML rename map overrides names. |
| Errors | Typed error enums via `thiserror` in library crates, `Result` everywhere, no panics on hardware errors. The CLI and examples may use `anyhow`. |

## Open questions

- Rename-map file format and location.
- How the singleton is enforced across threads and after `Drop` (can a design be reloaded?).
- Reload semantics: what happens to driver handles when a new bitstream is loaded.
