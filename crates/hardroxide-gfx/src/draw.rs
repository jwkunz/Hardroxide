use crate::font::{CELL_W, GLYPH_H, GLYPH_W, glyph};

/// A pixel: `0x00RRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Color = Color(0x000000);
    pub const WHITE: Color = Color(0xFFFFFF);
    pub const RED: Color = Color(0xFF0000);
    pub const GREEN: Color = Color(0x00FF00);
    pub const BLUE: Color = Color(0x0000FF);
    pub const YELLOW: Color = Color(0xFFFF00);
    pub const CYAN: Color = Color(0x00FFFF);
    pub const MAGENTA: Color = Color(0xFF00FF);
    pub const GRAY: Color = Color(0x808080);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color((r as u32) << 16 | (g as u32) << 8 | b as u32)
    }

    pub const fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub const fn b(self) -> u8 {
        self.0 as u8
    }

    /// `self` over `bg` with opacity `alpha` (0 = all `bg`, 255 = all `self`).
    pub fn over(self, bg: Color, alpha: u8) -> Color {
        let a = alpha as u32;
        let mix = |f: u8, b: u8| ((f as u32 * a + b as u32 * (255 - a) + 127) / 255) as u8;
        Color::rgb(
            mix(self.r(), bg.r()),
            mix(self.g(), bg.g()),
            mix(self.b(), bg.b()),
        )
    }

    /// A colour of the rainbow: `hue` 0 to 255 goes red, yellow, green, cyan, blue, magenta, red.
    pub const fn hue(hue: u8) -> Color {
        let h = hue as u32 * 6;
        let (sector, f) = (h / 256, (h % 256) as u8);
        let (up, down) = (f, 255 - f);
        match sector {
            0 => Color::rgb(255, up, 0),
            1 => Color::rgb(down, 255, 0),
            2 => Color::rgb(0, 255, up),
            3 => Color::rgb(0, down, 255),
            4 => Color::rgb(up, 0, 255),
            _ => Color::rgb(255, 0, down),
        }
    }
}

impl From<u32> for Color {
    fn from(v: u32) -> Self {
        Color(v & 0xFF_FFFF)
    }
}

/// A read-only block of pixels to draw from (a sprite, an image).
#[derive(Debug, Clone, Copy)]
pub struct Bitmap<'a> {
    pub pixels: &'a [u32],
    pub width: usize,
    pub height: usize,
    /// Pixels between the starts of lines (the width for a packed image).
    pub stride: usize,
}

impl<'a> Bitmap<'a> {
    /// A packed bitmap (stride equal to the width).
    ///
    /// # Panics
    /// If `pixels` is shorter than `width * height`.
    pub fn new(pixels: &'a [u32], width: usize, height: usize) -> Self {
        assert!(pixels.len() >= width * height, "pixel buffer too small");
        Bitmap {
            pixels,
            width,
            height,
            stride: width,
        }
    }
}

/// A drawing surface over a pixel buffer.
pub struct Surface<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> Surface<'a> {
    /// A surface of `width` x `height` pixels over `pixels` (at least that many).
    ///
    /// # Panics
    /// If the buffer is too small.
    pub fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
        Self::with_stride(pixels, width, height, width)
    }

    /// As [`Surface::new`] with `stride` pixels between the starts of lines.
    ///
    /// # Panics
    /// If the stride is smaller than the width or the buffer is too small.
    pub fn with_stride(pixels: &'a mut [u32], width: usize, height: usize, stride: usize) -> Self {
        assert!(stride >= width, "stride below width");
        assert!(
            height == 0 || pixels.len() >= stride * (height - 1) + width,
            "pixel buffer too small"
        );
        Surface {
            pixels,
            width,
            height,
            stride,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn stride(&self) -> usize {
        self.stride
    }
    pub fn pixels(&self) -> &[u32] {
        self.pixels
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// The pixel at `(x, y)`, or `None` off the surface.
    pub fn get(&self, x: i32, y: i32) -> Option<Color> {
        self.inside(x, y)
            .then(|| Color(self.pixels[y as usize * self.stride + x as usize]))
    }

    /// Set one pixel; ignored off the surface.
    pub fn set(&mut self, x: i32, y: i32, c: Color) {
        if self.inside(x, y) {
            self.pixels[y as usize * self.stride + x as usize] = c.0;
        }
    }

    pub fn clear(&mut self, c: Color) {
        for y in 0..self.height {
            let row = y * self.stride;
            self.pixels[row..row + self.width].fill(c.0);
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, len: i32, c: Color) {
        if y < 0 || y as usize >= self.height || len <= 0 {
            return;
        }
        let x0 = x.max(0);
        let x1 = (x + len).min(self.width as i32);
        if x1 > x0 {
            let row = y as usize * self.stride;
            self.pixels[row + x0 as usize..row + x1 as usize].fill(c.0);
        }
    }

    pub fn vline(&mut self, x: i32, y: i32, len: i32, c: Color) {
        for dy in 0..len.max(0) {
            self.set(x, y + dy, c);
        }
    }

    /// A line from `(x0, y0)` to `(x1, y1)` inclusive (Bresenham).
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.hline(x, y, w, c);
        self.hline(x, y + h - 1, w, c);
        self.vline(x, y, h, c);
        self.vline(x + w - 1, y, h, c);
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        for dy in 0..h.max(0) {
            self.hline(x, y + dy, w, c);
        }
    }

    /// A circle outline of radius `r` (midpoint algorithm).
    pub fn circle(&mut self, cx: i32, cy: i32, r: i32, c: Color) {
        if r < 0 {
            return;
        }
        let (mut x, mut y, mut d) = (r, 0, 1 - r);
        while x >= y {
            for (px, py) in [
                (x, y),
                (y, x),
                (-y, x),
                (-x, y),
                (-x, -y),
                (-y, -x),
                (y, -x),
                (x, -y),
            ] {
                self.set(cx + px, cy + py, c);
            }
            y += 1;
            if d < 0 {
                d += 2 * y + 1;
            } else {
                x -= 1;
                d += 2 * (y - x) + 1;
            }
        }
    }

    pub fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, c: Color) {
        if r < 0 {
            return;
        }
        let r2 = r * r;
        for dy in -r..=r {
            let dx = isqrt(r2 - dy * dy);
            self.hline(cx - dx, cy + dy, 2 * dx + 1, c);
        }
    }

    /// A filled triangle.
    pub fn fill_triangle(&mut self, a: (i32, i32), b: (i32, i32), c: (i32, i32), col: Color) {
        let mut v = [a, b, c];
        v.sort_unstable_by_key(|p| p.1);
        let [(x0, y0), (x1, y1), (x2, y2)] = v;
        for y in y0..=y2 {
            // The long edge from the top vertex to the bottom one, and the short edge on this side of the middle vertex.
            let xa = lerp_x(x0, y0, x2, y2, y);
            let xb = if y < y1 {
                lerp_x(x0, y0, x1, y1, y)
            } else {
                lerp_x(x1, y1, x2, y2, y)
            };
            let (l, r) = if xa < xb { (xa, xb) } else { (xb, xa) };
            self.hline(l, y, r - l + 1, col);
        }
    }

    /// Copy `src` to `(x, y)`. Pixels equal to `key` are skipped when it is `Some`.
    pub fn blit(&mut self, x: i32, y: i32, src: &Bitmap, key: Option<Color>) {
        for sy in 0..src.height {
            for sx in 0..src.width {
                let p = src.pixels[sy * src.stride + sx];
                if key.is_some_and(|k| k.0 == p & 0xFF_FFFF) {
                    continue;
                }
                self.set(x + sx as i32, y + sy as i32, Color(p));
            }
        }
    }

    /// As [`Surface::blit`] with every pixel blended over what is there at `alpha`.
    pub fn blit_alpha(&mut self, x: i32, y: i32, src: &Bitmap, alpha: u8) {
        for sy in 0..src.height {
            for sx in 0..src.width {
                let (px, py) = (x + sx as i32, y + sy as i32);
                if let Some(bg) = self.get(px, py) {
                    self.set(
                        px,
                        py,
                        Color(src.pixels[sy * src.stride + sx]).over(bg, alpha),
                    );
                }
            }
        }
    }

    /// Draw one character with its top-left corner at `(x, y)`, each font pixel `scale` pixels
    /// square. `bg` fills the cell behind it when given. Returns the cell width drawn.
    pub fn char(
        &mut self,
        x: i32,
        y: i32,
        ch: char,
        fg: Color,
        bg: Option<Color>,
        scale: i32,
    ) -> i32 {
        let s = scale.max(1);
        if let Some(b) = bg {
            self.fill_rect(x, y, CELL_W as i32 * s, GLYPH_H as i32 * s, b);
        }
        let g = glyph(ch);
        for (col, bits) in g.iter().enumerate().take(GLYPH_W) {
            for row in 0..GLYPH_H {
                if bits >> row & 1 == 1 {
                    self.fill_rect(x + col as i32 * s, y + row as i32 * s, s, s, fg);
                }
            }
        }
        CELL_W as i32 * s
    }

    /// Draw `text` from `(x, y)`; `'\n'` starts a new line. Returns the x position after the last
    /// character of the last line.
    pub fn text(
        &mut self,
        x: i32,
        y: i32,
        text: &str,
        fg: Color,
        bg: Option<Color>,
        scale: i32,
    ) -> i32 {
        let s = scale.max(1);
        let (mut cx, mut cy) = (x, y);
        for ch in text.chars() {
            if ch == '\n' {
                cx = x;
                cy += GLYPH_H as i32 * s;
            } else {
                cx += self.char(cx, cy, ch, fg, bg, s);
            }
        }
        cx
    }

    /// Width in pixels of `text`'s longest line at `scale`.
    pub fn text_width(text: &str, scale: i32) -> i32 {
        let longest = text
            .split('\n')
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        longest as i32 * CELL_W as i32 * scale.max(1)
    }
}

fn isqrt(n: i32) -> i32 {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// The x on the edge from `(xa, ya)` to `(xb, yb)` at row `y` (the edge's end if it is horizontal).
fn lerp_x(xa: i32, ya: i32, xb: i32, yb: i32, y: i32) -> i32 {
    if yb == ya {
        return xb;
    }
    xa + (xb - xa) * (y - ya) / (yb - ya)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::String, vec, vec::Vec};

    fn render(w: usize, h: usize, f: impl FnOnce(&mut Surface)) -> String {
        let mut buf = vec![0u32; w * h];
        f(&mut Surface::new(&mut buf, w, h));
        let mut s = String::new();
        for y in 0..h {
            for x in 0..w {
                s.push(if buf[y * w + x] == 0 { '.' } else { '#' });
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn colour_channels_and_blending() {
        let c = Color::rgb(1, 2, 3);
        assert_eq!((c.r(), c.g(), c.b(), c.0), (1, 2, 3, 0x010203));
        assert_eq!(Color::WHITE.over(Color::BLACK, 0), Color::BLACK);
        assert_eq!(Color::WHITE.over(Color::BLACK, 255), Color::WHITE);
        assert_eq!(
            Color::WHITE.over(Color::BLACK, 128),
            Color::rgb(128, 128, 128)
        );
    }

    #[test]
    fn the_hue_wheel_walks_the_primaries() {
        assert_eq!(Color::hue(0), Color::RED);
        assert_eq!(Color::hue(43), Color::rgb(253, 255, 0)); // just short of yellow
        assert_eq!(Color::hue(255).r(), 255);
    }

    #[test]
    fn lines_are_inclusive_and_symmetric() {
        let a = render(6, 6, |s| s.line(0, 0, 5, 5, Color::WHITE));
        assert_eq!(a, "#.....\n.#....\n..#...\n...#..\n....#.\n.....#\n");
        let b = render(6, 3, |s| s.line(5, 2, 0, 0, Color::WHITE));
        let c = render(6, 3, |s| s.line(0, 0, 5, 2, Color::WHITE));
        assert_eq!(b, c);
        assert_eq!(c.matches('#').count(), 6); // one pixel per column
    }

    #[test]
    fn rectangles_and_clipping() {
        assert_eq!(
            render(5, 4, |s| s.rect(1, 1, 3, 3, Color::WHITE)),
            ".....\n.###.\n.#.#.\n.###.\n"
        );
        assert_eq!(
            render(4, 3, |s| s.fill_rect(-2, 1, 4, 5, Color::WHITE)),
            "....\n##..\n##..\n"
        );
        // Wholly off-screen: nothing happens, no panic.
        assert_eq!(
            render(3, 3, |s| s.fill_rect(10, 10, 5, 5, Color::WHITE)),
            "...\n...\n...\n"
        );
        assert_eq!(
            render(3, 3, |s| s.line(-50, -50, 50, 50, Color::WHITE)),
            "#..\n.#.\n..#\n"
        );
    }

    #[test]
    fn circles() {
        let a = render(9, 9, |s| s.circle(4, 4, 3, Color::WHITE));
        assert_eq!(a.matches('#').count(), 16);
        let f = render(9, 9, |s| s.fill_circle(4, 4, 3, Color::WHITE));
        assert_eq!(f.lines().nth(4).unwrap(), ".#######.");
        assert!(f.matches('#').count() > a.matches('#').count());
    }

    #[test]
    fn a_filled_triangle_covers_its_corners_and_not_outside() {
        let t = render(8, 8, |s| {
            s.fill_triangle((0, 0), (7, 0), (0, 7), Color::WHITE)
        });
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "########");
        assert_eq!(lines[7], "#.......");
        assert_eq!(lines[3].chars().next(), Some('#'));
        assert_eq!(lines[3].chars().nth(7), Some('.'));
    }

    #[test]
    fn blits_honour_the_colour_key_and_stride() {
        let src = [1, 0, 2, 0, 3, 0]; // a 3 x 2 block
        let mut buf = vec![9u32; 4 * 3];
        let mut s = Surface::new(&mut buf, 4, 3);
        s.blit(1, 0, &Bitmap::new(&src, 3, 2), Some(Color(0)));
        assert_eq!(&buf[..4], &[9, 1, 9, 2]);
        assert_eq!(&buf[4..8], &[9, 9, 3, 9]);
    }

    #[test]
    fn alpha_blits_mix_with_the_background() {
        let mut buf = vec![0x000000u32; 1];
        let mut s = Surface::new(&mut buf, 1, 1);
        s.blit_alpha(0, 0, &Bitmap::new(&[0xFFFFFF], 1, 1), 128);
        assert_eq!(buf[0], 0x808080);
    }

    #[test]
    fn text_draws_glyphs_with_a_one_pixel_gap() {
        // 'I' is a vertical bar with serifs in the 5x7 font; two of them are six pixels apart.
        let t = render(12, 8, |s| {
            let end = s.text(0, 0, "II", Color::WHITE, None, 1);
            assert_eq!(end, 12);
        });
        let first: Vec<&str> = t.lines().map(|l| &l[..6]).collect();
        let second: Vec<&str> = t.lines().map(|l| &l[6..]).collect();
        assert_eq!(first, second);
        assert!(t.contains('#'));
        assert_eq!(Surface::text_width("abc\nde", 2), 3 * 6 * 2);
    }

    #[test]
    fn scaled_text_and_backgrounds() {
        let mut buf = vec![0u32; 12 * 16];
        let mut s = Surface::new(&mut buf, 12, 16);
        s.char(0, 0, '.', Color::WHITE, Some(Color(0x0000FF)), 2);
        // The cell is 12 x 16: the background fills it all, the dot's pixels are white.
        assert_eq!(
            buf.iter().filter(|&&p| p == 0x0000FF).count()
                + buf.iter().filter(|&&p| p == 0xFFFFFF).count(),
            12 * 16
        );
        assert!(buf.contains(&0xFFFFFF));
    }

    #[test]
    fn strides_leave_the_padding_alone() {
        let mut buf = vec![7u32; 6 * 2];
        let mut s = Surface::with_stride(&mut buf, 4, 2, 6);
        s.clear(Color(0));
        assert_eq!(&buf[..6], &[0, 0, 0, 0, 7, 7]);
    }
}
