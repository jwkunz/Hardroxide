# Applications 5: Ethernet and a web server

**Board:** [PYNQ-Z2](../../boards/pynq-z2.md) · **Runs:** bare metal and RTOS (the real-time kernel)

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 1](01-base-lights.md), an Ethernet cable from the board to a PC (or a router)

> **Status: run on a PYNQ-Z2 (2026-11-21)** with the board cabled straight to a PC: link at 1000 Mbit/s, an address from the PC's DHCP server, `ping`, a web server (status page, JSON, LED control from
> `curl`, a generated picture), and an RTOS variant that serves 21,879 requests in 35 seconds while the 10 ms control loop keeps its deadline to 1 microsecond. **Not run:** a browser (everything was `curl` and Python),
> 10 and 100 Mbit/s links, a router in between (only the direct cable). The first run found a bug in the PS configuration (section 7).

The Zynq's Ethernet MAC (GEM 0) is driven by [`ps7::gem`](../../reference/ps7.md#ethernet-mac-gem); [`hardroxide-net`](../../reference/net.md) is the protocol stack, written for this project:
ARP, IPv4, ICMP echo, UDP, DHCP, TCP and HTTP/1.1. The two meet at a four-method `Device` trait, so the stack also runs in host tests and under QEMU. There is no third-party code.

## 1. The pieces

| Piece | Where | What it does |
|---|---|---|
| `Gem` | [`hardroxide-ps7`](../../reference/ps7.md#ethernet-mac-gem) | the MAC: descriptor rings, frames in and out, MDIO, link negotiation (feature `net` adds the `Device` impl) |
| `Stack` | [`hardroxide-net`](../../reference/net.md) | addresses (DHCP or static), ARP, ping, UDP and TCP sockets |
| `Server` | `hardroxide_net::http` | accept, parse, call your handler, send, close |

## 2. Answer a ping

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal -o work/base --hardroxide-path . --name design_1
$ cd work/base
$ cargo add --dev --path ../../crates/hardroxide-net
$ sed -i 's/"panic-handler"\]/"panic-handler", "net"]/' Cargo.toml      # the MAC's Device impl
$ cp ../../apps/base/baremetal/net_ping.rs examples/
$ cargo build --release --example net_ping
$ hardroxide run target/armv7a-none-eabihf/release/examples/net_ping --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 50
```

Plug the cable in first. The program brings up the MAC, waits for the PHY to negotiate, and asks DHCP for an address (a PC that shares its connection runs a DHCP server; with none the board
takes 192.168.2.100 after eight seconds, for a PC port at 192.168.2.1/24):

```console
== net_ping ==
waiting for the link (plug in the cable)...
PHY 0: 1000 Mbit/s full duplex
NET PING READY: address 192.168.2.135 (from DHCP) - ping it
frames in 21 out 10, echo replies 5, bad 0, dropped 0
```

and from the PC:

```console
$ ping -c 5 192.168.2.135
5 packets transmitted, 5 received, 0% packet loss, time 4082ms
rtt min/avg/max/mdev = 0.239/0.284/0.341/0.033 ms
```

```rust,ignore
let mut gem = unsafe { Gem::new(be.map(ps7::map::GEM0_BASE as u64, ps7::map::GEM_SIZE).unwrap(), DMA_BASE, mac) };
gem.init().unwrap();
let link = gem.link_up(&slcr, 15_000).unwrap();       // PHY, autonegotiation, speed, clock; 1000 Mbit/s here
let mut net: Stack<_, 1, 1, 1024> = Stack::new(gem, Config::Dhcp { fallback: Some(static_config), fallback_after_ms: 8000 }, now_ms());
loop {
    net.poll(now_ms());                 // frames in, timers, frames out
    // ... use net.tcp(i), net.udp(i) here
}
```

`DMA_BASE` is a MiB of DDR (`0x0F00_0000`) made non-cacheable with `ps7::mem::set_non_cacheable`, because the MAC's DMA reads and writes the descriptor rings and frame buffers behind the processor's cache.

## 3. UDP: an echo and a log sink

`net_udp.rs` echoes datagrams on port 7 and prints datagrams sent to port 9999, so the board can collect log lines: `echo "hello" | nc -u -w1 192.168.2.135 9999` prints `hello` on the console. It is two `udp(i).bind(port)` calls and a `recv_from`/`udp_send` in the loop.

## 4. A web server

```console
$ cp ../../apps/base/baremetal/net_web.rs examples/
$ cargo add --dev --path ../../crates/hardroxide-gfx
$ cargo build --release --example net_web
$ hardroxide run target/armv7a-none-eabihf/release/examples/net_web --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 90
```

```console
== net_web ==
NET WEB READY: http://192.168.2.135/ (from DHCP)
```

From the PC:

```console
$ curl http://192.168.2.135/api/status
{"ip":"192.168.2.135","uptime_ms":9811,"leds":0,"switches":1,"buttons":0,"requests":0,"rx_frames":6,"tx_frames":6,"tcp_segments_in":3,"tcp_resets_sent":0}
$ curl "http://192.168.2.135/led?v=5"
LEDs 0101
$ curl -o picture.bmp http://192.168.2.135/picture.bmp      # 320x120, drawn by hardroxide-gfx on each request: uptime and the LEDs as circles
```

`/` is a page with a button per LED (it polls `/api/status` once a second); the handler is a `match` on the method and path:

```rust,ignore
server.poll(&mut net, |req, res| match (req.method, req.path) {
    ("GET", "/") => res.bytes(200, "text/html; charset=utf-8", PAGE),
    ("GET" | "POST", "/led") => match req.param("v").and_then(|v| v.parse::<u32>().ok()) {
        Some(v) if v < 16 => { leds.write(Channel::One, v).unwrap(); res.text(200, "text/plain", format_args!("LEDs {v:04b}\n")) }
        _ => res.text(400, "text/plain", format_args!("use /led?v=N with N from 0 to 15\n")),
    },
    ("GET", "/picture.bmp") => res.generated(200, "image/bmp", len, bmp_bytes, 0),   // produced in pieces as the connection has room
    _ => res.text(404, "text/plain", format_args!("not found: {}\n", req.path)),
});
```

**Measured** from the PC over the direct cable (Python `urllib`, a new connection per request, 200 requests):

| | |
|---|---|
| one client, one request after another | **3,193 requests/s** (0.3 ms each), 200 of 200 succeeded |
| five clients at once | **5,051 requests/s**, 200 of 200 succeeded |
| `/picture.bmp` (115 KB, rendered per request) | 7.2 MB/s |

(The page `/` is a 1 KB document; these are not file-serving figures. The picture's rate is limited by the byte-at-a-time BMP encoder, not by the network.)

## 5. The RTOS variant: a web server and a 10 ms control loop

`apps/base/rtos/rt_net.rs` runs three tasks on the [`hardroxide-rt`](../../reference/rt.md) kernel: `control` (priority 5, every 10 ms, records its own release jitter), `network` (priority 3, owns the MAC and the
stack and polls every millisecond; its status page shows the control loop's numbers) and `report` (priority 2, prints). It has to be generated with `--platform rtos`.

```console
control: 1875 releases, worst deviation from 10 ms 1 us; web: http://192.168.2.135/ 0 requests          <- no load
...
load: 21879 requests in 35 s = 625/s from 6 clients, 0 errors
control: 5877 releases, worst deviation from 10 ms 1 us; web: http://192.168.2.135/ 21879 requests     <- after the load
```

Six clients asking for the status page as fast as they could (625 requests/s, limited by the Python clients and by the network task's millisecond poll) did not change the control loop's worst deviation from
10 ms: **1 microsecond over about 5,900 releases**, the same as with the board idle. Because the network task is below the control task in priority and the control task sleeps between releases, the web server can
use only the time the control loop leaves.

## 6. Without the board: QEMU

`scripts/test-qemu.sh` runs the same stack on QEMU's emulated MAC with its user-mode network (a virtual router that answers DHCP and forwards ports), and talks to it from the host with real `curl` and Python sockets:
DHCP gives 10.0.2.15; a page; a 404; a 300 kB download compared byte for byte; a POST echoed; 40 requests in a row; 6 at once; a UDP echo; a 100 kB TCP echo. That is the check that the stack agrees with
other implementations of the protocols, not just with itself.

## 7. What the first run taught us

1. **The PS configuration left the Ethernet pins unconnected.** The first run found no PHY: every MDIO read returned 0. Reading the registers over JTAG showed `MIO_PIN_16` to `27` and `52`, `53` at `0x1201`
   (tri-stated, no peripheral) and the MAC's clock source on the external EMIO input, although the board preset said "Ethernet 0 on MIO 16 to 27". Setting the preset's keys in Vivado one at a time, in alphabetical order, reset the
   pin group to EMIO when the peripheral was enabled after it. Applying the preset in two passes fixed it, and with a regenerated `ps7_init` the PHY answered at once (a Realtek RTL8211E, ID `0x001cc915`).
2. **A server that refuses when it is full loses clients.** With three server sockets and six clients, the three extra connections were refused with a reset in the QEMU test, and `curl` gave up. A busy port now ignores
   the SYN, so the client retries a second later, as with a listen backlog (only a port nobody serves draws a reset).
3. **TIME-WAIT does not suit a small server.** A finished connection waited two seconds in `TIME-WAIT`, so a server with a few sockets ran out within moments. The server now takes a socket back for listening at once.
4. **The regression script had been skipping the video and audio programs** since 2.2: it built every example with no extra crates, the build failed, and the blocks after it were skipped, so "10 passed" was not
   the whole list. It now adds the crates first.

## 8. What is not covered

A browser; 10 and 100 Mbit/s links; a router or switch between the board and the PC (DHCP was the PC's); a cable unplugged and plugged in while running; sustained traffic for hours (the soak is for 2.7); the second MAC; and anything
past a trusted network: the stack has no defences against hostile traffic and its sequence numbers are not unpredictable. HTTPS is the next version.
