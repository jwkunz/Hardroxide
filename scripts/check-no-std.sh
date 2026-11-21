#!/usr/bin/env bash
# The runtime crates must stay usable without `std`, allocation and an operating
# system. This builds them for a bare-metal ARM target (the Zynq-7000's Cortex-A9)
# with default features off: any use of `std` fails to compile.
set -euo pipefail
rustup target add armv7a-none-eabihf >/dev/null
CRATES="-p hardroxide-mmio -p hardroxide-fpga -p hardroxide-dma -p hardroxide-irq -p hardroxide-ip -p hardroxide-gfx -p hardroxide-video -p hardroxide-audio -p hardroxide-fat"
cargo build --release --target armv7a-none-eabihf --no-default-features $CRATES "$@"
# The PS support crate (ARM only parts included).
cargo build --release --target armv7a-none-eabihf -p hardroxide-ps7 --features panic-handler "$@"
# The kernel (ARM only parts included).
cargo build --release --target armv7a-none-eabihf -p hardroxide-rt "$@"
# `alloc` adds the Vec-returning conveniences and needs a global allocator at link time,
# so only check that it type-checks.
cargo check --release --target armv7a-none-eabihf --no-default-features --features hardroxide-ip/alloc -p hardroxide-ip "$@"
echo "no_std build ok"
