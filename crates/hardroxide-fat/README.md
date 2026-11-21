# hardroxide-fat

A FAT32 file system over a block device (an SD card, a disk image): mount, directories with long names, read and write files,
create and delete, and format a whole card. `no_std`, no allocation, no dependencies beyond the Hardroxide block trait.

This is a crate of [Hardroxide](https://github.com/jwkunz/Hardroxide): a generator and runtime that turns a Vivado XSA into a typed
Rust HAL crate for Zynq-7000 (validated on a PYNQ-Z2). The user guide and API
reference are at <https://jwkunz.github.io/Hardroxide/>.

Licensed under either of Apache-2.0 or MIT at your option.
