# On-board tests of the generated Linux crates for Zynq UltraScale+

The Linux counterparts of [`../board_tests_rfsoc`](../board_tests_rfsoc/README.md): one program for each RFSoC 4x2
design but the radio one (`base` is `fixtures/board/rfsoc4x2_base_overlay.xsa`, the others
`fixtures/rfsoc4x2/rfsoc4x2_<name>.xsa`). Copy a program into the `examples/` of the generated crate, cross-build it and
run it on the board as root, after `board_check`; `loopback` and `dma` need their `interrupts.dtso` applied first (one
after the other in one boot only with care: see the guide), `dma` and `hls_scale` need `u-dma-buf` loaded. Each prints
`<NAME> OK`. How: [Linux on Zynq UltraScale+](../../docs/guide/linux-ultrascale.md).
`cargo test -p hardroxide-gen --test linux_psu` type-checks them.
