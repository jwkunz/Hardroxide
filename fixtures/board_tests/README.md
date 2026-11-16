# On-board tests

Programs used to validate the generated crates on a PYNQ-Z2. Each directory is
named after a fixture (`fixtures/pynq_z2_<name>.xsa`). To run one: generate the
crate (`hardroxide generate fixtures/pynq_z2_<name>.xsa -o out --hardroxide-path .`),
copy the `.rs` file into `out/examples/`, cross-build with
`cargo build --release --target armv7-unknown-linux-gnueabihf --example <name>`
(see [Getting started](../../docs/guide/getting-started.md#build-for-the-board)),
copy the binary to the board and run it as root. Prepare the board first
([Running on hardware](../../docs/guide/hardware.md)). Results are recorded in
[Verification gaps](../../docs/08-verification-gaps.md).

Interrupt tests need the generated `interrupts.dtso` applied. A GIC line keeps
the trigger type (level or edge) of the first overlay that mapped it until
reboot, so switching to a design that needs the other type on the same line
requires a reboot.
