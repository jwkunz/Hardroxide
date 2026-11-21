# hardroxide-gfx

A small 2D graphics library over a pixel buffer (`&mut [u32]`, `0x00RRGGBB`): clipped pixels, lines, rectangles, circles,
triangles, blits with colour keys and alpha, and a built-in 5x7 bitmap font. `no_std`, no allocation; the only dependency is `thiserror`.

This is a crate of [Hardroxide](https://github.com/jwkunz/Hardroxide): a generator and runtime that turns a Vivado XSA into a typed
Rust HAL crate for Zynq-7000 (validated on a PYNQ-Z2). The user guide and API
reference are at <https://jwkunz.github.io/Hardroxide/>.

Licensed under either of Apache-2.0 or MIT at your option.
