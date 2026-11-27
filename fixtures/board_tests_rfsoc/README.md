# On-board tests of the generated crates for Zynq UltraScale+

One program for each RFSoC 4x2 design (`gpio`, ... are `fixtures/rfsoc4x2/rfsoc4x2_<name>.xsa`; `base` is
`fixtures/board/rfsoc4x2_base_overlay.xsa`), bare metal, using only the generated crate and the drivers. Each prints
`<NAME> OK` at the end; anything wrong stops it with a message. `scripts/test-board-rfsoc.sh` generates the crates,
copies the programs into their `examples/` and runs them; `cargo test -p hardroxide-gen --test board_tests_psu`
compiles them.
