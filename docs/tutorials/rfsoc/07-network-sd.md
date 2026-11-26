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

Starting a program from the card, without a host, is not supported on this family yet (`hardroxide package` handles the
Zynq-7000 only).

Next: [the base overlay under Linux](08-linux.md).
