# hardroxide-gfx

A small 2D graphics library over a pixel buffer. A `Surface` borrows a `&mut [u32]` of `0x00RRGGBB` pixels and draws on it. Every
operation takes signed coordinates and **clips** to the surface. `no_std`, no allocation, no dependencies; the tests render to memory
on the host and compare against expected pictures.

```rust,ignore
pub struct Color(pub u32);
pub const fn rgb(r: u8, g: u8, b: u8) -> Color
pub const fn r(self) -> u8
pub const fn g(self) -> u8
pub const fn b(self) -> u8
pub fn over(self, bg: Color, alpha: u8) -> Color
pub const fn hue(hue: u8) -> Color
pub struct Bitmap<'a>
pub fn new(pixels: &'a [u32], width: usize, height: usize) -> Self
pub struct Surface<'a>
pub fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self
pub fn with_stride(pixels: &'a mut [u32], width: usize, height: usize, stride: usize) -> Self
pub fn get(&self, x: i32, y: i32) -> Option<Color>
pub fn set(&mut self, x: i32, y: i32, c: Color)
pub fn clear(&mut self, c: Color)
pub fn hline(&mut self, x: i32, y: i32, len: i32, c: Color)
pub fn vline(&mut self, x: i32, y: i32, len: i32, c: Color)
pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color)
pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color)
pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color)
pub fn circle(&mut self, cx: i32, cy: i32, r: i32, c: Color)
pub fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, c: Color)
pub fn fill_triangle(&mut self, a: (i32, i32), b: (i32, i32), c: (i32, i32), col: Color)
pub fn blit(&mut self, x: i32, y: i32, src: &Bitmap, key: Option<Color>)
pub fn blit_alpha(&mut self, x: i32, y: i32, src: &Bitmap, alpha: u8)
pub fn char(&mut self, x: i32, y: i32, ch: char, fg: Color, bg: Option<Color>, scale: i32) -> i32
pub fn text(&mut self, x: i32, y: i32, text: &str, fg: Color, bg: Option<Color>, scale: i32) -> i32
pub fn text_width(text: &str, scale: i32) -> i32
pub fn glyph(c: char) -> &'static [u8; GLYPH_W]
```

- `Color` has the named colours `BLACK`, `WHITE`, `RED`, `GREEN`, `BLUE`, `YELLOW`, `CYAN`, `MAGENTA`, `GRAY`; `over` blends a colour over a
  background at an opacity; `hue(0..=255)` walks the rainbow.
- `Surface::new` panics if the buffer is smaller than `width * height`; `with_stride` allows padding between lines (the padding is never touched).
- `line` is Bresenham, endpoints included. `circle` is the midpoint algorithm. `fill_triangle` fills by scanlines.
- `Bitmap` is a read-only block (`width`, `height`, `stride`); `blit` copies it, skipping pixels equal to a colour key; `blit_alpha` blends.
- Text uses a built-in 5x7 font for ASCII 32 to 126 (anything else draws as `?`): a character cell is 6 x 8 pixels at `scale` 1; `text` returns the x after
  the last character and starts a new line at `'\n'`. The glyph table is the classic dot-matrix set, re-keyed for this crate and checked by rendering.
