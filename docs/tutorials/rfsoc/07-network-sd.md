# RFSoC 7: Network and SD card

The Ethernet and SD drivers are the Zynq-7000's, extended for this family; the [network stack](../../reference/net.md),
[TLS](../../reference/tls.md) and [file system](../../reference/fat.md) are unchanged.

## A web server

```console
$ cd baremetal/net-smoke
$ cargo build --release --target aarch64-unknown-none
$ hardroxide run target/aarch64-unknown-none/release/net-smoke \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 240
```

```text
== Ethernet and network stack smoke test ==
PHY at address 15: DP83867, clock delays set
PHY at address 15, link 1000 Mbit/s full duplex
NET READY ip=<address> dhcp=true gateway=Some([...])
```

While it runs, `python3 scripts/net_check.py <address> 80 9 443 7 --figures` (from the repository's root) checks HTTP,
HTTPS, TCP and UDP with `curl`, Python and `openssl`. The HTTPS identity is a **throwaway test key** kept in the
repository, the TLS code is unaudited and its random numbers are weak: [read this first](../../reference/tls.md).

On this board, cabled to a PC: about 18 MB/s over HTTP, about 220 requests a second and about 21 new TLS connections a
second.

## A web page on the kernel

`apps/rfsoc4x2/rtos/rt_dashboard.rs` serves a page for the base overlay (temperature, buttons, switches, LED controls)
as one task of the [real-time kernel](../../reference/rt.md), beside a control loop that must run every 10 ms. Generate
the kernel crate of [tutorial 4](04-mood-light.md), and add the network stack, and the Ethernet part of the processor's
crate, under `[dev-dependencies]` in its `Cargo.toml`:

```toml
hardroxide-net = { path = "../Hardroxide/crates/hardroxide-net" }
hardroxide-psu = { path = "../Hardroxide/crates/hardroxide-psu", default-features = false, features = ["net"] }
```

```console
$ cp apps/rfsoc4x2/rtos/rt_dashboard.rs ../base-rtos/examples/
$ cd ../base-rtos
$ cargo build --release --example rt_dashboard
$ hardroxide run target/aarch64-unknown-none/release/examples/rt_dashboard \
    --xsa ../Hardroxide/fixtures/board/rfsoc4x2_base_overlay.xsa --watch 100
```

```text
== RFSoC 4x2 base overlay: a web page beside a 10 ms control loop ==
link 1000 Mbit/s full duplex
RT DASHBOARD READY: http://<address>/
control: 3878 releases, worst deviation from 10 ms 1 us; web: http://<address>/ 10763 requests
```

Open the address in a browser. The program keeps serving after `hardroxide run` stops showing its console. **There is no
password**: whoever can reach the board can set its LEDs.

That last line was printed after four clients on the host had asked as fast as they could for 20 seconds: about 540
requests a second, every one answered, and the control loop never more than 1 us from its period.

| Task | Priority | Does |
|---|---|---|
| `control` | 5 | every 10 ms: the LEDs, switches, buttons and temperature; times itself |
| `network` | 3 | the Ethernet port and the stack, polled every millisecond; the page |
| `report` | 2 | prints the numbers every five seconds |

Only `control` touches the FPGA; the tasks meet in a few atomic numbers, so nothing a client does can make the loop
wait. Compare [tutorial 9](09-linux-web.md), the same page under Linux: there it is shorter, and nothing promises when
anything runs.

## The SD card

> **Warning.** `baremetal/sd-smoke` **erases the card in the slot**. It must be built for one card by serial number, and
> leaves any other card untouched.

First look, which cannot write:

```console
$ cd baremetal/sd-info
$ cargo build --release --target aarch64-unknown-none
$ hardroxide run target/aarch64-unknown-none/release/sd-info \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 15
```

It prints the card's serial number, partitions and files. Only if that card holds **nothing you want** (a card without a
FAT32 partition is not necessarily blank):

```console
$ cd ../sd-smoke
$ HRX_ERASE_SERIAL=0x........ cargo build --release --target aarch64-unknown-none
$ hardroxide run target/aarch64-unknown-none/release/sd-smoke \
    --xsa ../../fixtures/board/rfsoc4x2_base_overlay.xsa --watch 40
```

It formats the card, writes and reads back files and reports the speed. On a 16 GB card: 1.8 MB/s written and 6 MB/s read
in 4 KiB pieces, 11 MB/s in one large read, and the file system passed `fsck.fat`.

To start a program from a card, without a host, `hardroxide package` builds the `BOOT.BIN`
([Booting on its own](../../guide/baremetal.md#booting-on-its-own)). On this board a test program started that way, with
its FPGA design loaded.

Next: [the base overlay under Linux](08-linux.md).
