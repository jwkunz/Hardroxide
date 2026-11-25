#!/usr/bin/env bash
# The runtime crates must stay usable without `std`, allocation and an operating
# system. This builds them for the bare-metal ARM targets (the Zynq-7000's Cortex-A9
# and Zynq UltraScale+'s Cortex-A53) with default features off: any use of `std`
# fails to compile.
set -euo pipefail
rustup target add armv7a-none-eabihf >/dev/null
CRATES="-p hardroxide-mmio -p hardroxide-fpga -p hardroxide-dma -p hardroxide-irq -p hardroxide-ip -p hardroxide-gfx -p hardroxide-video -p hardroxide-audio -p hardroxide-fat -p hardroxide-net -p hardroxide-crypto -p hardroxide-tls"
cargo build --release --target armv7a-none-eabihf --no-default-features $CRATES "$@"
# The PS support crate (ARM only parts included).
cargo build --release --target armv7a-none-eabihf -p hardroxide-ps7 --features panic-handler "$@"
# The kernel (ARM only parts included).
cargo build --release --target armv7a-none-eabihf -p hardroxide-rt "$@"
# `alloc` adds the Vec-returning conveniences and needs a global allocator at link time,
# so only check that it type-checks.
cargo check --release --target armv7a-none-eabihf --no-default-features --features hardroxide-ip/alloc -p hardroxide-ip "$@"
# Zynq UltraScale+ (Cortex-A53, ADR 0005): the same libraries, and the portable parts of the PS crate and the kernel.
rustup target add aarch64-unknown-none >/dev/null
cargo build --release --target aarch64-unknown-none --no-default-features $CRATES "$@"
cargo build --release --target aarch64-unknown-none -p hardroxide-ps7 --features net "$@"
cargo build --release --target aarch64-unknown-none -p hardroxide-psu --features panic-handler "$@"
cargo build --release --target aarch64-unknown-none -p hardroxide-rt "$@"
echo "no_std build ok"
