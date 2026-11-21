//! A small 2D graphics library over a pixel buffer.
//!
//! A [`Surface`] borrows a `&mut [u32]` of `0x00RRGGBB` pixels (the format the base overlay's HDMI
//! pipeline shows) and draws on it: pixels, lines, rectangles, circles, triangles, blits with
//! colour keys and alpha, and text in a built-in 5x7 font. Every operation takes signed
//! coordinates and clips to the surface, so drawing partly or wholly off-screen is safe. No
//! allocation and no dependencies, so it runs on a processor with no operating system, and its
//! tests render to memory on the host.

#![cfg_attr(not(test), no_std)]

#[cfg(test)]
extern crate alloc;

mod draw;
pub mod font;

pub use draw::{Bitmap, Color, Surface};
