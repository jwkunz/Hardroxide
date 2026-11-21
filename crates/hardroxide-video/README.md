# hardroxide-video

HDMI output for the PYNQ base overlay: display modes (640x480 to 1920x1080 at 60 Hz) and the bring-up of the pixel
clock, the video timing generator, the colour and pixel-format converters and the frame buffer DMA, with double
buffering by switching the DMA between frame buffers. `no_std`.

This is a crate of [Hardroxide](https://github.com/jwkunz/Hardroxide): a generator and runtime that turns a Vivado XSA into a typed
Rust HAL crate for Zynq-7000 (validated on a PYNQ-Z2). The user guide and API
reference are at <https://jwkunz.github.io/Hardroxide/>.

Licensed under either of Apache-2.0 or MIT at your option.
